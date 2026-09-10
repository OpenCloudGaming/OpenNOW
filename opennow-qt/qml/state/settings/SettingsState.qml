import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var appController
    required property var i18n
    required property bool ready
    required property var subscription
    required property bool nativeRuntimeReady
    required property var nativeRuntimeCapabilities
    required property var refreshAccountServices
    required property var refreshStreamerDetection
    required property var syncDiscordPresence
    required property var syncTelemetry
    required property string lastError
    signal consoleSurfaceRequested(bool enabled)
    signal accessibilityAnnounced(string message)
    signal errorReported(string message)
    property var settings: ({})
    property string previewThemePack: ""
    property string settingsRequestId: ""
    property string consoleSurfaceRequestId: ""
    property bool consoleSurfaceConfirmedValue: false
    property bool consoleSurfaceDesiredValue: false
    property bool consoleSurfaceRequestValue: false
    property bool consoleSurfaceInitialized: false
    property string consoleSurfaceError: ""

    function refreshSettings() {
        if (!ready)
            return
        settingsRequestId = coreClient.request("settings.get", {})
    }

    function codecNamesFromCapabilities(capabilities, hdrOnly) {
        const result = []
        const backends = capabilities && capabilities.videoBackends
            ? capabilities.videoBackends : []
        for (let backendIndex = 0; backendIndex < backends.length; ++backendIndex) {
            const backend = backends[backendIndex]
            const requested = String(settings.nativeVideoBackend || "auto")
            if (!backend.available || ["software", "ffmpeg"].indexOf(backend.backend) >= 0
                    || (requested !== "auto" && requested !== backend.backend
                    && !(requested === "nvdec" && backend.backend === "cuda")))
                continue
            const codecs = backend.codecs || []
            for (let codecIndex = 0; codecIndex < codecs.length; ++codecIndex) {
                const codec = codecs[codecIndex]
                const name = String(codec.codec || "").toLowerCase()
                if (hdrOnly && (["h265", "av1"].indexOf(name) < 0
                        || (codec.hdrSupported !== undefined && codec.hdrSupported !== true)
                        || (Array.isArray(codec.colorQualities)
                            ? codec.colorQualities.indexOf("10bit_420") < 0 : codec.hdrSupported !== true)))
                    continue
                if (codec.available && ["h264", "h265", "av1"].indexOf(name) >= 0
                        && result.indexOf(name) < 0)
                    result.push(name)
            }
        }
        return result
    }

    function codecAvailable(codec) {
        const name = String(codec || "h264").toLowerCase()
        // Bind directly to current capabilities and backend preference. Do not leave old
        // support enabled while a reconnect or a new hardware probe is pending.
        return nativeRuntimeReady
            && codecNamesFromCapabilities(nativeRuntimeCapabilities).indexOf(name) >= 0
    }

    function hdrDecoderAvailable() {
        return nativeRuntimeReady && settings.decoderPreference !== "software"
            && codecNamesFromCapabilities(nativeRuntimeCapabilities, true).length > 0
    }

    function availableCodecValues() {
        const result = []
        if (codecAvailable("h264")) {
            result.push("auto")
            result.push("h264")
        }
        if (codecAvailable("h265"))
            result.push("h265")
        if (codecAvailable("av1"))
            result.push("av1")
        // Keep the persisted choice visible when the child is missing or the selected decoder
        // policy has no compatible codec. Prelaunch validation still rejects it before CloudMatch.
        return result.length ? result : [String(settings.codec || "auto").toLowerCase()]
    }

    function availableCodecLabels() {
        const values = availableCodecValues()
        const result = []
        for (let index = 0; index < values.length; ++index) {
            result.push(values[index] === "auto" ? "Auto"
                : values[index] === "h264" ? "H.264"
                : values[index] === "h265" ? "H.265" : "AV1")
        }
        return result
    }

    // Canonical frame rates offered by GeForce NOW clients. The Rust core
    // clamps fps to 30–240, so 360 (an Electron-legacy preset) is excluded.
    function canonicalFpsValues() {
        return [30, 60, 90, 120, 144, 165, 240]
    }

    // Official catalog rates per resolution. 1080p rigs offer 240 FPS;
    // other modes top out at 120 FPS. Exact MES tuples (e.g. 90 FPS) are
    // preserved separately and never synthesized from a higher envelope.
    function presetFpsForResolution(width, height) {
        if (width === 1920 && (height === 1080 || height === 1200))
            return [30, 60, 120, 240]
        return [30, 60, 120]
    }

    function isFpsCoveredByEntitlement(width, height, fps) {
        const raw = (subscription && subscription.entitledResolutions) || []
        for (let index = 0; index < raw.length; ++index) {
            if (Number(raw[index].width || 0) >= width
                    && Number(raw[index].height || 0) >= height
                    && Number(raw[index].fps || 0) >= fps)
                return true
        }
        return false
    }

    // Frame rates the membership entitles at the given resolution, mirroring
    // Electron's getFpsForResolution. Returns [] when no exact entitlement
    // tuple exists or no subscription is loaded (callers fall back to the
    // offline static list with nothing locked).
    function entitledFpsForResolution(resolution) {
        const parts = String(resolution || "").split("x")
        const width = Number(parts[0])
        const height = Number(parts[1])
        if (!(width > 0 && height > 0) || !subscription)
            return []
        const exact = []
        const raw = subscription.entitledResolutions || []
        for (let index = 0; index < raw.length; ++index) {
            if (Number(raw[index].width) === width && Number(raw[index].height) === height) {
                const fps = Math.trunc(Number(raw[index].fps || 0))
                if (fps >= 30 && exact.indexOf(fps) < 0)
                    exact.push(fps)
            }
        }
        if (exact.length === 0)
            return []
        const presets = presetFpsForResolution(width, height)
        for (let index = 0; index < presets.length; ++index) {
            if (exact.indexOf(presets[index]) < 0
                    && isFpsCoveredByEntitlement(width, height, presets[index]))
                exact.push(presets[index])
        }
        exact.sort((left, right) => left - right)
        return exact
    }

    // Canonical rates NOT entitled at the given resolution. Empty when the
    // subscription is unknown so offline users keep the full static list.
    function unentitledFpsValues(resolution) {
        const entitled = entitledFpsForResolution(resolution)
        if (entitled.length === 0)
            return []
        const locked = []
        const canonical = canonicalFpsValues()
        for (let index = 0; index < canonical.length; ++index) {
            if (entitled.indexOf(canonical[index]) < 0)
                locked.push(canonical[index])
        }
        return locked
    }

    // Clamp a requested fps to the nearest entitled rate at or below it,
    // mirroring resolveEntitledStreamProfile. Returns the input when the
    // subscription is unknown.
    function resolveEntitledFps(resolution, requested) {
        const entitled = entitledFpsForResolution(resolution)
        if (entitled.length === 0)
            return requested
        const wanted = Math.trunc(Number(requested || 0))
        if (wanted === 0 || entitled.indexOf(wanted) >= 0)
            return wanted
        for (let index = entitled.length - 1; index >= 0; --index) {
            if (entitled[index] <= wanted)
                return entitled[index]
        }
        return entitled[0]
    }

    // Persistently correct settings.fps when the resolution or subscription
    // changed underneath it (e.g. tier downgrade). No-op while offline.
    function clampFpsToEntitlement() {
        const clamped = resolveEntitledFps(settings.resolution, settings.fps)
        if (Number(clamped) !== Number(settings.fps))
            setSetting("fps", clamped)
    }

    function resolutionItems() {
        const groups = [
            ["16:9 STANDARD", [["720p","1280x720"],["900p","1600x900"],["1080p","1920x1080"],["1440p","2560x1440"],["1800p","3200x1800"],["4K","3840x2160"],["5K","5120x2880"],["8K","7680x4320"]]],
            ["16:10 WIDESCREEN", [["800p","1280x800"],["900p","1440x900"],["1050p","1680x1050"],["1200p","1920x1200"],["1600p","2560x1600"],["2400p","3840x2400"]]],
            ["21:9 ULTRAWIDE", [["UW 1080p","2560x1080"],["UW 1440p","3440x1440"],["UW 1600p","3840x1600"],["UW 1800p","3840x1800"],["UW 2160p","5120x2160"]]],
            ["32:9 SUPER ULTRAWIDE", [["Dual 1080p","3840x1080"],["Dual 1440p","5120x1440"]]]
        ]
        const items = []
        for (const group of groups) {
            items.push({kind:"heading",label:group[0]})
            for (const option of group[1])
                items.push({kind:"choice",label:option[0],detail:option[1].replace("x","×"),value:option[1],
                    disabled: Boolean(root.subscription) && root.entitledFpsForResolution(option[1]).length === 0})
        }
        return items
    }

    function videoBackendItems() {
        const backends = nativeRuntimeCapabilities.videoBackends || []
        const result = [{label: qsTr("Auto (recommended)"), value: "auto",
            detail: qsTr("Use a supported native backend")}]
        const choices = Qt.platform.os === "windows"
            ? [{label:"DirectX 11", value:"d3d11"}, {label:"DirectX 12", value:"d3d12"}, {label:"Vulkan", value:"vulkan"}]
            : Qt.platform.os === "osx"
                ? [{label:"Metal / VideoToolbox", value:"videotoolbox"}]
                : backends.filter(backend => ["vulkan", "cuda", "vaapi", "v4l2"].indexOf(backend.backend) >= 0)
                    .map(backend => ({label:String(backend.backend).toUpperCase(), value:backend.backend}))
        for (const choice of choices) {
            const backend = backends.find(backend => backend.backend === choice.value)
            result.push(Object.assign({}, choice, {
                disabled: !nativeRuntimeReady || !backend || !backend.available,
                detail: !nativeRuntimeReady ? qsTr("Checking hardware…")
                    : !backend ? qsTr("Not supported by this stream view")
                    : backend.available ? qsTr("Hardware decoding")
                    : String(backend.reason || qsTr("Unavailable on this device"))
            }))
        }
        return result
    }

    function requestConsoleSurface(enabled) {
        const requested = Boolean(enabled)
        const previous = consoleSurfaceInitialized
            ? consoleSurfaceConfirmedValue : Boolean(settings.launchInConsoleMode)
        consoleSurfaceError = ""
        consoleSurfaceDesiredValue = requested
        applySetting("launchInConsoleMode", requested)
        root.consoleSurfaceRequested(requested)
        if (!ready) {
            consoleSurfaceDesiredValue = previous
            applySetting("launchInConsoleMode", previous)
            root.consoleSurfaceRequested(previous)
            consoleSurfaceError = qsTr("Console mode could not be saved because the OpenNOW core is not ready. The previous mode was restored.")
            errorReported(consoleSurfaceError)
            accessibilityAnnounced(lastError)
            return ""
        }
        if (consoleSurfaceRequestId === "")
            beginConsoleSurfacePersistence()
        return consoleSurfaceRequestId
    }

    function beginConsoleSurfacePersistence() {
        consoleSurfaceRequestValue = consoleSurfaceDesiredValue
        consoleSurfaceRequestId = coreClient.request("settings.set", {
            key: "launchInConsoleMode",
            value: consoleSurfaceRequestValue
        })
        if (consoleSurfaceRequestId !== "")
            return
        const restored = consoleSurfaceConfirmedValue
        consoleSurfaceDesiredValue = restored
        applySetting("launchInConsoleMode", restored)
        root.consoleSurfaceRequested(restored)
        consoleSurfaceError = qsTr("Console mode could not be saved. The previous mode was restored.")
        errorReported(consoleSurfaceError)
        accessibilityAnnounced(lastError)
    }

    function setSetting(key, value) {
        if (key === "launchInConsoleMode")
            return requestConsoleSurface(Boolean(value))
        if (!ready) {
            errorReported(qsTr("The OpenNOW core is not ready"))
            return ""
        }
        const requestId = coreClient.request("settings.set", { key: key, value: value })
        if (key === "identifyAsSteamDeck") {
            // MES serves a different resolution catalog per device identity
            // (Steam Deck unlocks 90 FPS tuples), so re-read entitlements.
            Qt.callLater(root.refreshAccountServices)
        }
        return requestId
    }

    function resetSettings() {
        if (!ready) {
            errorReported(qsTr("The OpenNOW core is not ready"))
            return
        }
        coreClient.request("settings.reset", {})
    }

    function applyCoupledSettings(changes) {
        for (const key of Object.keys(changes || {}))
            root.applySetting(key, changes[key])
    }

    function applySetting(key, value) {
        const updated = Object.assign({}, settings)
        updated[key] = value
        if (key === "themePack") {
            updated.appTheme = value === "bone" || value === "cobalt" ? "light" : "dark"
            updated.themeAccentOverride = false
        } else if (key === "appAccentColor") {
            updated.themeAccentOverride = true
        }
        settings = updated
        if (["nativeStreamerExecutablePath", "nativeVideoBackend", "decoderPreference"].indexOf(key) >= 0)
            Qt.callLater(root.refreshStreamerDetection)
        accessibilityAnnounced(qsTr("%1 updated").arg(String(key).split(/(?=[A-Z])/).join(" ")))
        if (key === "reducedMotion")
            appController.reducedMotion = Boolean(value)
        if (key === "appLanguage")
            i18n.setLocale(String(value || "system"))
        if (key === "discordRichPresence")
            syncDiscordPresence()
        if (key === "errorReportingConsent")
            syncTelemetry()
    }

    function acceptSettings(result) {
        root.settings = Object.assign({}, result.settings)
        root.consoleSurfaceConfirmedValue = Boolean(result.settings.launchInConsoleMode)
        root.consoleSurfaceDesiredValue = root.consoleSurfaceConfirmedValue
        root.consoleSurfaceInitialized = true
        appController.reducedMotion = Boolean(result.settings.reducedMotion)
        i18n.setLocale(String(result.settings.appLanguage || "system"))
        root.settingsRequestId = ""
    }

    function acceptConsoleSurface(result) {
        root.applyCoupledSettings(result.changes)
        root.consoleSurfaceRequestId = ""
        root.consoleSurfaceConfirmedValue = Boolean(result.value)
        root.consoleSurfaceInitialized = true
        if (root.consoleSurfaceDesiredValue === root.consoleSurfaceConfirmedValue) {
            root.applySetting("launchInConsoleMode", root.consoleSurfaceConfirmedValue)
            root.consoleSurfaceError = ""
            accessibilityAnnounced(root.consoleSurfaceConfirmedValue
                ? qsTr("Console mode saved") : qsTr("Computer mode saved"))
        } else {
            root.beginConsoleSurfacePersistence()
        }
    }

    function failConsoleSurface(message) {
        root.consoleSurfaceRequestId = ""
        root.consoleSurfaceDesiredValue = root.consoleSurfaceConfirmedValue
        root.applySetting("launchInConsoleMode", root.consoleSurfaceConfirmedValue)
        root.consoleSurfaceRequested(root.consoleSurfaceConfirmedValue)
        root.consoleSurfaceError = qsTr("Console mode could not be saved. The previous mode was restored. %1").arg(message)
        errorReported(root.consoleSurfaceError)
        accessibilityAnnounced(root.lastError)
    }

    function acceptSettingsChange(payload) {
        // Coupled preferences are saved atomically by the core.
        root.applyCoupledSettings(payload.changes)
        if (payload.key === "launchInConsoleMode") {
            const persisted = Boolean(payload.value)
            root.consoleSurfaceConfirmedValue = persisted
            root.consoleSurfaceInitialized = true
            if (root.consoleSurfaceRequestId !== ""
                    && root.consoleSurfaceDesiredValue !== persisted)
                return
            root.consoleSurfaceDesiredValue = persisted
            root.applySetting(payload.key, persisted)
            root.consoleSurfaceRequested(persisted)
        } else {
            root.applySetting(payload.key, payload.value)
        }
    }
}

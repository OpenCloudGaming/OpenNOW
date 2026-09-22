import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var appController
    required property var i18n
    required property bool ready
    required property var subscription
    required property var authSession
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
    property string providerIdpId: ""
    property string providerCode: ""
    readonly property string selectedRegion: {
        const saved = settings.providerRegions || {}
        if (saved[providerIdpId] !== undefined)
            return String(saved[providerIdpId])
        if (settings.regionProviderIdpId === providerIdpId
                || (!settings.regionProviderIdpId && providerCode === "NVIDIA"))
            return String(settings.region || "")
        return ""
    }
    property string previewThemePack: ""
    property string settingsRequestId: ""
    property string consoleSurfaceRequestId: ""
    property bool consoleSurfaceConfirmedValue: false
    property bool consoleSurfaceDesiredValue: false
    property bool consoleSurfaceRequestValue: false
    property bool consoleSurfaceInitialized: false
    property string consoleSurfaceError: ""
    property double scopeGeneration: 0
    property bool nativeHdrOutputSupported: false
    property bool settingsActive: false
    property bool capabilitiesActive: false
    property var keyboardLayouts: []
    property var languageResult: ({})
    property string languageState: "idle"
    property string languageError: ""
    property string languageRequestId: ""
    property var colorDescriptors: []
    property var codecDescriptors: []
    property string colorRequestId: ""
    property var frameRateDescriptors: []
    property string cancellingRequestId: ""
    property var settingWrites: ({})
    property string shortcutUpdateRequestId: ""
    property string shortcutUpdateError: ""
    readonly property string languageContext: JSON.stringify([ready, scopeGeneration,
        providerIdpId, settings.sessionProxyEnabled, settings.sessionProxyUrl])
    readonly property string colorContext: JSON.stringify([ready, nativeRuntimeReady,
        nativeRuntimeCapabilities, nativeHdrOutputSupported, settings.codec, settings.colorQuality, settings.nativeVideoBackend,
        settings.decoderPreference, settings.enableHdr, settings.resolution])
    readonly property string gameLanguageDescription: qsTr("Requested when the game supports it; some games require an in-game change. Applies to the next session.")
    readonly property string keyboardLayoutDescription: qsTr("Physical key mapping requested from GeForce NOW. Applies to the next session.")
    readonly property string interfaceLanguageDescription: qsTr("OpenNOW interface only. Community translated through Crowdin.")
    readonly property string colorDescription: qsTr("Availability follows the current backend, codec and HDR output. Saved unsupported choices are preserved; launch validates the profile.")
    readonly property bool softwareDecodeRequested: {
        const backend = String(settings.nativeVideoBackend || "auto")
        if (["auto", ""].indexOf(backend) < 0)
            return ["software", "ffmpeg"].indexOf(backend) >= 0
        return settings.decoderPreference === "software"
    }

    readonly property string languageStatusText: {
        if (languageState === "loading") return qsTr("Loading game languages… Saved preferences are unchanged.")
        if (languageState === "stale") return qsTr("Using stale cached game languages. %1").arg(languageError)
        if (languageState === "error") return qsTr("Game language metadata unavailable. Saved preferences are unchanged. %1").arg(languageError)
        if (languageState === "success") return languageResult.cacheHit === true
            ? qsTr("Using cached game languages. This is not per-game support or entitlement.")
            : qsTr("Global game languages. This is not per-game support or entitlement.")
        return qsTr("Game language metadata has not been loaded. Saved preferences are unchanged.")
    }
    readonly property var interfaceLanguageItems: {
        const revision = i18n ? i18n.revision : 0
        const locales = i18n ? i18n.availableLocales : ["en"]
        return ["system"].concat(locales).map(value => ({value:value,
            label:i18n ? i18n.localeDisplayName(value) : value}))
    }
    readonly property var gameLanguageItems: {
        const revision = i18n ? i18n.revision : 0
        const values = languageResult.languages || []
        const items = values.map(value => ({value:value,
            label:i18n ? i18n.localeDisplayName(value) : value,
            detail:languageState === "stale" ? qsTr("Stale cached metadata") : ""}))
        if (!items.length) items.push({value:"en_US", label:i18n ? i18n.localeDisplayName("en_US") : "en_US", detail:qsTr("Local fallback; support not confirmed")})
        const saved = String(settings.gameLanguage || "en_US")
        if (!values.includes(saved)) {
            const existing = items.find(item => item.value === saved)
            const detail = values.length ? qsTr("Saved; not listed in current metadata") : qsTr("Saved; support not confirmed")
            if (existing) existing.detail = detail
            else items.unshift({value:saved, label:saved, detail:detail, disabled:true})
        }
        return items
    }
    readonly property var keyboardLayoutItems: {
        const items = keyboardLayouts.map(item => ({value:item.value, label:i18n ? i18n.source(item.label) : item.label}))
        const saved = String(settings.keyboardLayout || "en-US")
        if (!items.some(item => item.value === saved)) {
            const alias = keyboardLayouts.find(item => (item.aliases || []).includes(saved))
            items.unshift({value:saved, label:saved, disabled:true,
                detail:alias ? qsTr("Saved legacy ID; requests %1").arg(alias.value)
                    : keyboardLayouts.length ? qsTr("Saved; layout not recognized") : qsTr("Saved; keyboard choices unavailable")})
        }
        return items
    }
    readonly property var colorQualityItems: [
        ["8bit_420", qsTr("8-bit, YUV 4:2:0")], ["8bit_444", qsTr("8-bit, YUV 4:4:4")],
        ["10bit_420", qsTr("10-bit, YUV 4:2:0")], ["10bit_444", qsTr("10-bit, YUV 4:4:4")]
    ].map(pair => {
        const descriptor = colorDescriptors.find(item => item.value === pair[0])
        const gateReason = colorQualityGate(pair[0])
        const blocked = gateReason !== "" || !descriptor || descriptor.disabled
        return {value:pair[0], label:pair[1], disabled:blocked,
            detail: gateReason !== ""
                ? gateReason
                : descriptor ? String(descriptor.reason || qsTr("Supported by the current profile")) : qsTr("Capability not confirmed")}
    })

    function membershipTierName() {
        const tier = (subscription && subscription.membershipTier)
            || (authSession && authSession.user && authSession.user.membershipTier)
            || ""
        return String(tier).toUpperCase()
    }

    // 10-bit color precision and HDR10 require Ultimate or Performance.
    // Unknown tiers fail open; the service remains the final arbiter.
    function tenBitAllowedByMembership() {
        return membershipTierName() !== "FREE"
    }

    function fourFourFourAllowedByMembership() {
        const tier = membershipTierName()
        return tier === "" || tier === "ULTIMATE"
    }

    // Membership/platform gates mirroring GeForce NOW's documented
    // availability. Returns "" when allowed, else a display reason.
    function colorQualityGate(value) {
        const quality = String(value || "")
        if (quality.indexOf("444") >= 0) {
            if (Qt.platform.os !== "windows" && Qt.platform.os !== "osx")
                return qsTr("Available in the Windows and macOS apps")
            if (!fourFourFourAllowedByMembership())
                return qsTr("Requires an Ultimate membership")
        }
        if (quality.indexOf("10bit") === 0 && !tenBitAllowedByMembership())
            return qsTr("Requires a Performance or Ultimate membership")
        return ""
    }

    onLanguageContextChanged: {
        const request = languageRequestId
        languageRequestId = ""
        languageDeadline.stop()
        languageResult = ({})
        languageError = ""
        languageState = "idle"
        cancelOwnedRequest(request)
        if (settingsActive && ready) Qt.callLater(root.ensureGameLanguages)
    }
    onSettingsActiveChanged: if (settingsActive) ensureGameLanguages()
    onCapabilitiesActiveChanged: if (capabilitiesActive) colorRefresh.restart()
    onColorContextChanged: {
        const request = colorRequestId
        colorRequestId = ""
        colorDescriptors = []
        codecDescriptors = []
        frameRateDescriptors = []
        cancelOwnedRequest(request)
        if (capabilitiesActive) colorRefresh.restart()
    }
    property Timer languageDeadline: Timer {
        interval: 15000
        onTriggered: {
            const request = root.languageRequestId
            root.languageRequestId = ""
            root.languageState = (root.languageResult.languages || []).length ? "stale" : "error"
            root.languageError = qsTr("The request timed out. Retry when ready.")
            root.cancelOwnedRequest(request)
        }
    }
    property Timer colorRefresh: Timer {
        interval: 0
        onTriggered: {
            if (!root.ready || !root.nativeRuntimeReady || !root.capabilitiesActive || root.colorRequestId !== "") return
            root.colorRequestId = root.coreClient.request("settings.choices.get", {runtimeCapabilities:root.nativeRuntimeCapabilities}, 15000)
        }
    }

    function ensureGameLanguages(refresh) {
        const expired = languageState === "success" && Number(languageResult.expiresAt || 0) <= Date.now()
        if (!ready || languageRequestId !== "" || (!refresh && languageState !== "idle" && !expired)) return
        languageState = "loading"
        languageError = ""
        languageRequestId = coreClient.request("catalog.languages.get", {refresh:refresh === true}, 15000)
        if (languageRequestId !== "") languageDeadline.restart()
        else { languageState = "error"; languageError = qsTr("The request could not be started.") }
    }

    function cancelOwnedRequest(id) {
        if (id === "") return
        cancellingRequestId = id
        coreClient.cancel(id)
        cancellingRequestId = ""
    }

    function ownsConfirmedSetting(key) {
        return ["appLanguage", "gameLanguage", "keyboardLayout", "colorQuality", "codec",
            "nativeVideoBackend", "decoderPreference", "enableHdr"].includes(key)
    }

    function beginSettingWrite(key, value) {
        const writes = Object.assign({}, settingWrites)
        if (writes[key]) {
            writes[key] = Object.assign({}, writes[key], {next:value, queued:true})
            settingWrites = writes
            return writes[key].id
        }
        const id = coreClient.request("settings.set", {key:key, value:value}, 15000)
        if (id === "") { errorReported(qsTr("The setting could not be saved.")); return "" }
        writes[key] = {id:id, value:value, queued:false}
        settingWrites = writes
        return id
    }

    function finishSettingWrite(id, result, message) {
        const key = Object.keys(settingWrites).find(key => settingWrites[key].id === id)
        if (!key) return false
        const write = settingWrites[key]
        const writes = Object.assign({}, settingWrites)
        delete writes[key]
        settingWrites = writes
        if (result) {
            // Coupled values first (per protocol): core repairs persisted together
            // with the primary key, e.g. an explicit codec the new color mode
            // cannot use is healed toward Auto in the same save.
            applyCoupledSettings(result.changes)
            applySetting(key, result.value)
        } else if (!write.queued) errorReported(message)
        if (write.queued && ready) beginSettingWrite(key, write.next)
        return true
    }

    function acceptResponse(id, result) {
        if (id !== "" && id === languageRequestId) {
            languageRequestId = ""
            languageDeadline.stop()
            if (Number(result.scopeGeneration) !== scopeGeneration) {
                languageState = "error"
                languageError = qsTr("The language metadata scope changed. Retry when ready.")
                return true
            }
            languageResult = result
            languageState = String(result.status || "error")
            languageError = String(result.error && (result.error.message || result.error) || "")
            return true
        }
        if (id !== "" && id === colorRequestId) {
            colorRequestId = ""
            colorDescriptors = result.colorQualities || []
            codecDescriptors = result.codecs || []
            frameRateDescriptors = result.frameRates || []
            clampFpsToEntitlement()
            return true
        }
        if (id !== "" && id === shortcutUpdateRequestId) {
            applyCoupledSettings(result.bindings)
            shortcutUpdateRequestId = ""
            return true
        }
        return finishSettingWrite(id, result, "")
    }

    function acceptFailure(id, message) {
        if (id !== "" && id === cancellingRequestId) return true
        if (id !== "" && id === shortcutUpdateRequestId) {
            shortcutUpdateError = message || qsTr("The shortcut could not be saved.")
            shortcutUpdateRequestId = ""
            accessibilityAnnounced(shortcutUpdateError)
            return true
        }
        if (id !== "" && id === languageRequestId) {
            languageRequestId = ""
            languageDeadline.stop()
            languageState = (languageResult.languages || []).length ? "stale" : "error"
            languageError = message
            return true
        }
        if (id !== "" && id === colorRequestId) {
            colorRequestId = ""
            colorDescriptors = []
            codecDescriptors = []
            frameRateDescriptors = []
            return true
        }
        return finishSettingWrite(id, null, message)
    }

    function refreshSettings() {
        if (!ready)
            return
        settingsRequestId = coreClient.request("settings.get", {})
    }

    function codecNamesFromCapabilities(capabilities, hdrOnly) {
        const result = []
        const backends = capabilities && capabilities.videoBackends
            ? capabilities.videoBackends : []
        const requested = String(settings.nativeVideoBackend || "auto")
        const softwareRequested = softwareDecodeRequested
        for (let backendIndex = 0; backendIndex < backends.length; ++backendIndex) {
            const backend = backends[backendIndex]
            const software = ["software", "ffmpeg"].indexOf(backend.backend) >= 0
            if (!backend.available || (softwareRequested ? !software : software)
                    || (!softwareRequested && requested !== "auto" && requested !== backend.backend
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

    function codecDescriptor(codec) {
        const name = String(codec || "").toLowerCase()
        for (let index = 0; index < codecDescriptors.length; ++index) {
            if (String(codecDescriptors[index].value || "").toLowerCase() === name)
                return codecDescriptors[index]
        }
        return null
    }

    // Core-resolved gating for the current color quality, backend, and HDR mode.
    // Fails open while descriptors are unconfirmed: decoder capability gating
    // (codecAvailable) still applies, and launch validation rejects the rest.
    function codecDisabledByProfile(codec) {
        const descriptor = codecDescriptor(codec)
        return descriptor !== null && descriptor.disabled === true
    }

    function codecsDisabledByProfile() {
        return ["av1", "h264", "h265"].filter(codec => codecDisabledByProfile(codec))
    }

    function hdrDecoderAvailable() {
        return nativeRuntimeReady && settings.decoderPreference !== "software"
            && codecNamesFromCapabilities(nativeRuntimeCapabilities, true).length > 0
    }

    function canonicalFpsValues() {
        return [30, 60, 90, 120, 144, 165, 240, 360]
    }

    function presetFpsForResolution(width, height) {
        if (width === 1920 && (height === 1080 || height === 1200))
            return [30, 60, 120, 240, 360]
        return [30, 60, 120]
    }

    readonly property var capabilityGatedFpsValues: [360]

    function frameRateDescriptor(value) {
        for (let index = 0; index < frameRateDescriptors.length; ++index) {
            if (Number(frameRateDescriptors[index].value) === Number(value))
                return frameRateDescriptors[index]
        }
        return null
    }

    function frameRateGated(value) {
        return capabilityGatedFpsValues.indexOf(Number(value)) >= 0
    }

    function frameRateEligible(value) {
        const descriptor = frameRateDescriptor(value)
        if (descriptor !== null)
            return descriptor.disabled !== true
        return !frameRateGated(value)
    }

    function maxEntitledFps(resolution) {
        const entitled = entitledFpsForResolution(resolution)
        return entitled.length ? entitled[entitled.length - 1] : 0
    }

    function frameRateReason(value) {
        const descriptor = frameRateDescriptor(value)
        return descriptor && descriptor.disabled === true ? String(descriptor.reason || "") : ""
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

    function selectableFpsValues(resolution) {
        const locked = lockedFpsValues(resolution)
        return canonicalFpsValues().filter(value => locked.indexOf(value) < 0)
    }

    function knownLockedFpsValues(resolution) {
        const locked = unentitledFpsValues(resolution)
        const canonical = canonicalFpsValues()
        for (let index = 0; index < canonical.length; ++index) {
            const descriptor = frameRateDescriptor(canonical[index])
            if (descriptor && descriptor.disabled === true && locked.indexOf(canonical[index]) < 0)
                locked.push(canonical[index])
        }
        return locked
    }

    function lockedFpsValues(resolution) {
        const locked = knownLockedFpsValues(resolution)
        const canonical = canonicalFpsValues()
        const entitled = entitledFpsForResolution(resolution)
        for (let index = 0; index < canonical.length; ++index) {
            const value = canonical[index]
            const unconfirmed = frameRateGated(value)
                && (entitled.indexOf(value) < 0 || !frameRateEligible(value))
            if (unconfirmed && locked.indexOf(value) < 0)
                locked.push(value)
        }
        return locked
    }

    function lockedFpsReason() {
        const canonical = canonicalFpsValues()
        for (let index = 0; index < canonical.length; ++index) {
            const reason = frameRateReason(canonical[index])
            if (reason !== "")
                return reason
        }
        const locked = lockedFpsValues(settings.resolution)
        for (let index = 0; index < locked.length; ++index) {
            if (frameRateGated(locked[index]))
                return qsTr("Capability not confirmed")
        }
        return ""
    }

    function resolveEntitledFps(resolution, requested) {
        const locked = knownLockedFpsValues(resolution)
        const selectable = canonicalFpsValues().filter(value => locked.indexOf(value) < 0)
        if (selectable.length === 0)
            return requested
        const wanted = Math.trunc(Number(requested || 0))
        if (wanted === 0 || selectable.indexOf(wanted) >= 0)
            return wanted
        for (let index = selectable.length - 1; index >= 0; --index) {
            if (selectable[index] <= wanted)
                return selectable[index]
        }
        return selectable[0]
    }

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
        if (Qt.platform.os !== "windows" && Qt.platform.os !== "osx")
            choices.push({label: qsTr("Software (CPU)"), value: "software"})
        for (const choice of choices) {
            const backend = backends.find(backend => backend.backend
                === (choice.value === "software" ? "ffmpeg" : choice.value))
            result.push({label: choice.label, value: choice.value,
                disabled: !nativeRuntimeReady || !backend || !backend.available,
                detail: !nativeRuntimeReady ? qsTr("Checking hardware…")
                    : !backend ? qsTr("Not supported by this stream view")
                    : backend.available ? (choice.value === "software"
                        ? qsTr("CPU decoding; 8-bit 4:2:0 SDR only") : qsTr("Hardware decoding"))
                    : String(backend.reason || qsTr("Unavailable on this device"))
            })
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
        if (ownsConfirmedSetting(key)) return beginSettingWrite(key, value)
        const requestId = coreClient.request("settings.set", { key: key, value: value, providerIdpId: providerIdpId })
        if (key === "identifyAsSteamDeck") {
            // MES serves a different resolution catalog per device identity
            // (Steam Deck unlocks 90 FPS tuples), so re-read entitlements.
            Qt.callLater(root.refreshAccountServices)
        }
        return requestId
    }

    function updateShortcuts(bindings) {
        if (shortcutUpdateRequestId !== "")
            return ""
        shortcutUpdateError = ""
        if (!ready) {
            shortcutUpdateError = qsTr("The OpenNOW core is not ready")
            return ""
        }
        shortcutUpdateRequestId = coreClient.request("settings.shortcuts.update", {bindings: bindings}, 15000)
        if (shortcutUpdateRequestId === "")
            shortcutUpdateError = qsTr("The shortcut could not be saved.")
        return shortcutUpdateRequestId
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
        root.keyboardLayouts = result.keyboardLayouts || []
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
        if (root.settingWrites[payload.key]) return
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

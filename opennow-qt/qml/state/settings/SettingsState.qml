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
    required property string lastError
    signal consoleSurfaceRequested(bool enabled)
    signal accessibilityAnnounced(string message)
    signal errorReported(string message)
    property var settings: ({})
    property var confirmedSettings: ({})
    readonly property bool forwardFlightControls: confirmedSettings.forwardFlightControls !== false
    property var awdlController: MacAwdl
    property Binding awdlPreferenceBinding: Binding {
        target: root.awdlController
        property: "enabled"
        value: root.confirmedSettings.suppressAwdlWhileStreaming === true
    }
    property string providerIdpId: ""
    property string providerCode: ""
    readonly property string selectedRegion: {
        const write = settingWrites.region
        if (write) {
            const provider = write.queued ? write.nextProviderIdpId : write.providerIdpId
            const generation = write.queued ? write.nextScopeGeneration : write.scopeGeneration
            if (provider === providerIdpId && generation === scopeGeneration)
                return String(write.queued ? write.next : write.value)
        }
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
    property double settingWriteSequence: 0
    property string shortcutUpdateRequestId: ""
    property string shortcutUpdateError: ""
    readonly property string languageContext: JSON.stringify([ready, scopeGeneration,
        providerIdpId, settings.sessionProxyEnabled, settings.sessionProxyUrl])
    readonly property string colorContext: JSON.stringify([ready, nativeRuntimeReady,
        nativeRuntimeCapabilities, nativeHdrOutputSupported, settings.codec, settings.colorQuality, settings.nativeVideoBackend,
        settings.decoderPreference, settings.enableHdr, settings.resolution,
        settings.webrtcCompatibilityMode, providerIdpId, providerCode])
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
            detail: pair[0].endsWith("_444") ? qsTr("Coming soon") : gateReason !== ""
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
    onReadyChanged: {
        if (!ready) {
            const writes = settingWrites
            settingWrites = ({})
            for (const key of Object.keys(writes)) {
                if (!ownsConfirmedSetting(key) && key !== "gameCollections")
                    reconcileSetting(key, writes[key].coupledKeys)
            }
        }
    }
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
            "nativeVideoBackend", "decoderPreference", "enableHdr", "webrtcCompatibilityMode",
            "forwardFlightControls"].includes(key)
    }

    function beginSettingWrite(key, value, previousWrite, source) {
        const writes = Object.assign({}, settingWrites)
        if (writes[key]) {
            writes[key] = Object.assign({}, writes[key], {next:value, nextSource:source, queued:true,
                nextProviderIdpId:providerIdpId, nextScopeGeneration:scopeGeneration,
                sequence:++settingWriteSequence})
            settingWrites = writes
            return writes[key].id
        }
        const id = coreClient.request("settings.set", Object.assign({key:key, value:value, providerIdpId:providerIdpId},
            source ? {source:source} : {}), 15000)
        if (id === "") { errorReported(qsTr("The setting could not be saved.")); return "" }
        const coupledKeys = previousWrite ? previousWrite.coupledKeys
            : Object.keys(settingValues(key, value)).filter(changedKey => changedKey !== key)
        const baseline = Object.assign({}, confirmedSettings)
        for (const changedKey of [key].concat(coupledKeys)) {
            if (!Object.prototype.hasOwnProperty.call(baseline, changedKey))
                baseline[changedKey] = settings[changedKey]
        }
        confirmedSettings = baseline
        writes[key] = {id:id, value:value, queued:false, providerIdpId:providerIdpId,
            scopeGeneration:scopeGeneration,
            sequence:previousWrite ? previousWrite.sequence : ++settingWriteSequence,
            coupledKeys:coupledKeys,
            initialValue:previousWrite ? previousWrite.initialValue : confirmedSettings[key]}
        settingWrites = writes
        return id
    }

    function finishSettingWrite(id, result, message) {
        const key = Object.keys(settingWrites).find(key => settingWrites[key].id === id)
        if (!key) return false
        const write = settingWrites[key]
        const confirmed = ownsConfirmedSetting(key)
        const dispatchNext = write.queued && ready && (key !== "region"
            || (write.nextProviderIdpId === providerIdpId && write.nextScopeGeneration === scopeGeneration))
        const writes = Object.assign({}, settingWrites)
        delete writes[key]
        settingWrites = writes
        if (result) {
            confirmedSettings = Object.assign({}, confirmedSettings, result.changes || {}, {[key]:result.value})
            write.coupledKeys = Array.from(new Set(write.coupledKeys.concat(Object.keys(result.changes || {}))))
        }
        if (result && confirmed) {
            // Coupled values first (per protocol): core repairs persisted together
            // with the primary key, e.g. an explicit codec the new color mode
            // cannot use is healed toward Auto in the same save.
            applyCoupledSettings(result.changes)
            applySetting(key, result.value)
        } else if (!result && confirmed && !dispatchNext) errorReported(message)
        const nextId = dispatchNext ? beginSettingWrite(key, write.next, write, write.nextSource) : ""
        if (!nextId) {
            if (!confirmed && key !== "gameCollections")
                reconcileSetting(key, write.coupledKeys)
            if (key === "identifyAsSteamDeck" && confirmedSettings[key] !== write.initialValue)
                Qt.callLater(root.refreshAccountServices)
        }
        return confirmed
    }

    function reconcileSetting(key, coupledKeys) {
        const keys = [key].concat(coupledKeys)
        const values = ({})
        for (const changedKey of keys)
            values[changedKey] = confirmedSettings[changedKey]
        const pendingKeys = Object.keys(settingWrites).sort((a, b) => settingWrites[a].sequence - settingWrites[b].sequence)
        for (const pendingKey of pendingKeys) {
            if (ownsConfirmedSetting(pendingKey) || pendingKey === "gameCollections") continue
            const pending = settingWrites[pendingKey]
            const changes = settingValues(pendingKey, pending.queued ? pending.next : pending.value)
            for (const changedKey of keys) {
                if (Object.prototype.hasOwnProperty.call(changes, changedKey))
                    values[changedKey] = changes[changedKey]
            }
        }
        for (const changedKey of keys)
            applySetting(changedKey, values[changedKey])
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

    function hdrNoticeShown(gameId) {
        return (settings.hdrNoticeShownGameIds || []).indexOf(gameId) >= 0
    }

    function rememberHdrNotice(gameId) {
        if (!gameId || hdrNoticeShown(gameId))
            return
        setSetting("hdrNoticeShownGameIds", (settings.hdrNoticeShownGameIds || []).concat([gameId]).slice(-200))
    }

    function hdrDecoderAvailable() {
        return nativeRuntimeReady && settings.decoderPreference !== "software"
            && codecNamesFromCapabilities(nativeRuntimeCapabilities, true).length > 0
    }

    // ---- Frame generation: levels, target FPS, display refresh and capability profiles ----
    // The shell window binds the refresh rate of the screen OpenNOW is on; a saved
    // frameGenerationDisplayHz overrides it when detection is wrong or reports 0.
    property real detectedDisplayHz: 0
    property string frameGenerationGpu: ""
    property string frameGenerationTestError: ""
    readonly property var frameGenerationLevels: ["2x", "3x", "4x"]
    readonly property var frameGenerationDisplayChoices: [60, 75, 90, 100, 120, 144, 165, 180, 240, 360]

    function frameGenerationLevelFactor(value) {
        const index = frameGenerationLevels.indexOf(String(value || "off"))
        return index < 0 ? 0 : index + 2
    }
    function frameGenerationResolution() {
        const text = String(settings.resolution || "1920x1080")
        const match = text.match(/^(\d+)x(\d+)$/)
        return match ? {width: Number(match[1]), height: Number(match[2]), text: text}
                     : {width: 1920, height: 1080, text: "1920x1080"}
    }
    // The stream frame rate generation works against: the selected FPS, or for Auto the
    // highest rate the membership allows at the selected resolution.
    function frameGenerationSourceFps() {
        const fps = Number(settings.fps || 0)
        if (fps > 0) return fps
        const selectable = selectableFpsValues(frameGenerationResolution().text)
        return selectable.length ? selectable[selectable.length - 1] : 60
    }
    function frameGenerationDisplayHz() {
        const override = Number(settings.frameGenerationDisplayHz || 0)
        return override > 0 ? override : Number(detectedDisplayHz || 0)
    }
    function frameGenerationTargetFps() { return Number(settings.frameGenerationTargetFps || 0) }
    // The level the saved settings ask for (0 = off); with a target FPS the level follows
    // the stream rate (round(target / source)).
    readonly property var frameGenerationModes: ["off", "multiplier", "target", "display"]
    // How the output rate is chosen: off, a fixed multiplier, a target FPS, or the display.
    function frameGenerationMode() {
        const mode = String(settings.frameGenerationMode || "")
        if (frameGenerationModes.indexOf(mode) >= 0) return mode
        return frameGenerationLevelFactor(settings.frameGeneration) ? "multiplier" : "off"
    }
    function frameGenerationTargetFactor(target) {
        const factor = Math.round(Number(target) / frameGenerationSourceFps())
        return factor >= 2 ? Math.min(4, factor) : 0
    }
    // Largest allowed multiplier the display can show for the stream rate (0 = none).
    function frameGenerationDisplayFactor() {
        const source = frameGenerationSourceFps(), display = frameGenerationDisplayHz()
        if (display <= 0) return 0
        for (let factor = 4; factor >= 2; --factor)
            if (source * factor <= display * 1.025 && frameGenerationAllowed(factor)) return factor
        return 0
    }
    function frameGenerationRequestedFactor() {
        switch (frameGenerationMode()) {
        case "multiplier": return frameGenerationLevelFactor(settings.frameGeneration)
        case "target": return frameGenerationTargetFactor(frameGenerationTargetFps())
        case "display": return frameGenerationDisplayFactor()
        default: return 0
        }
    }
    // The level that actually runs: frame generation is unavailable until the capability
    // test has passed for the current preset, and only the factors it allowed run.
    function frameGenerationFactor() {
        const factor = frameGenerationRequestedFactor()
        return factor >= 2 && frameGenerationAllowed(factor) ? factor : 0
    }
    // True once this GPU has been measured at the selected resolution and at least one
    // level fits the selected frame rate on the current display.
    function frameGenerationUnlocked() {
        return (frameGenerationVerdict().allowed || []).length > 0
    }
    function frameGenerationReasonText(reason) {
        switch (String(reason || "")) {
        case "display": return qsTr("Needs a faster display for this level")
        case "gpu": return qsTr("Too slow on this GPU for this level")
        case "engine": return qsTr("This engine generates 2× only")
        default: return ""
        }
    }
    function frameGenerationOutputFps() { return frameGenerationSourceFps() * frameGenerationFactor() }
    function ensureFrameGenerationGpu() {
        if (frameGenerationGpu === "") frameGenerationGpu = String(FrameGenerationBenchmark.gpuName() || "")
    }
    // The measured cost depends on the engine, the GPU and the stream resolution only;
    // whether a level fits is decided live from the selected frame rate and the display.
    function frameGenerationProfileKey() {
        return ["motion", frameGenerationGpu || "unknown-gpu", frameGenerationResolution().text].join("|")
    }
    function frameGenerationProfile() {
        const profiles = settings.frameGenerationProfiles || {}
        const key = frameGenerationProfileKey()
        if (profiles[key]) return profiles[key]
        // Measurements saved while the key still carried the frame rate stay valid.
        const legacy = Object.keys(profiles).find(name => name.indexOf(key + "|") === 0)
        return legacy ? profiles[legacy] : null
    }
    // {measured, allowed: [..], reasons: {"2": ""|"gpu"|"display"|"engine", ...}, costMs, budgetMs}
    function frameGenerationVerdictFor(fps) {
        const profile = frameGenerationProfile()
        if (!profile) return {measured: false, allowed: [], reasons: {}, costMs: {}, budgetMs: 0}
        // The engine maximum belongs to the installed engine: a profile measured by an older
        // engine keeps its 2x cost and extrapolates the levels it never timed.
        const verdict = FrameGenerationBenchmark.evaluate(
            Object.assign({ok: true}, profile, {engineMaxFactor: FrameGenerationBenchmark.engineMaxFactor()}),
            Number(fps), frameGenerationDisplayHz())
        verdict.measured = true
        return verdict
    }
    function frameGenerationVerdict() { return frameGenerationVerdictFor(frameGenerationSourceFps()) }
    // One row per canonical stream rate: which levels fit on this GPU and display. The cost
    // per source frame is the same for every rate, so one measurement fills the table.
    function frameGenerationMatrix() {
        const locked = lockedFpsValues(frameGenerationResolution().text)
        return canonicalFpsValues().map(fps => {
            const verdict = frameGenerationVerdictFor(fps)
            return {fps: fps, locked: locked.indexOf(fps) >= 0,
                    cells: [2, 3, 4].map(factor => ({
                        factor: factor, output: fps * factor,
                        allowed: (verdict.allowed || []).indexOf(factor) >= 0,
                        reason: String((verdict.reasons || {})[String(factor)] || ""),
                        costMs: Number((verdict.costMs || {})[String(factor)] || 0),
                        budgetMs: Number(verdict.budgetMs || 0)}))}
        })
    }
    function frameGenerationAllowed(factor) {
        return (frameGenerationVerdict().allowed || []).indexOf(Number(factor)) >= 0
    }
    function frameGenerationLockedValues() {
        return frameGenerationLevels.filter(value => !frameGenerationAllowed(frameGenerationLevelFactor(value)))
    }
    function frameGenerationLockedHint() {
        const verdict = frameGenerationVerdict()
        if (!verdict.measured) return qsTr("Run the capability test to unlock this level")
        for (let factor = 2; factor <= 4; ++factor) {
            const text = frameGenerationReasonText((verdict.reasons || {})[String(factor)])
            if (text !== "") return text
        }
        return ""
    }
    // Output rates the display can show at 2x..4x the stream rate.
    function frameGenerationTargetOptions() {
        const source = frameGenerationSourceFps(), display = frameGenerationDisplayHz()
        const options = []
        canonicalFpsValues().forEach(rate => {
            const factor = Math.round(rate / source)
            if (factor < 2 || factor > 4 || Math.abs(rate - factor * source) > source * 0.1) return
            if (display > 0 && rate > display * 1.025) return
            options.push({value: rate, label: String(rate), enabled: frameGenerationAllowed(factor), factor: factor})
        })
        return options
    }
    // Frame generation choices persist through the core like every other setting; fixtures
    // that run without a core keep the local map so the UI still follows the choice.
    function writeFrameGenerationSetting(key, value) {
        if (ready) setSetting(key, value)
        else applySetting(key, value)
    }
    // Keeps the stored level in step with the mode's factor so overlays and older readers agree.
    function storeFrameGenerationLevel(factor) {
        writeFrameGenerationSetting("frameGeneration", factor >= 2 ? frameGenerationLevels[Math.min(4, factor) - 2] : "off")
    }
    function selectFrameGenerationMode(mode) {
        mode = frameGenerationModes.indexOf(String(mode)) >= 0 ? String(mode) : "off"
        if (mode === "off") {
            writeFrameGenerationSetting("frameGeneration", "off")
            writeFrameGenerationSetting("frameGenerationMode", "off")
            return
        }
        if (mode === "multiplier") {
            if (frameGenerationLevelFactor(settings.frameGeneration) === 0) {
                const first = frameGenerationLevels.find(value => frameGenerationAllowed(frameGenerationLevelFactor(value)))
                if (first) writeFrameGenerationSetting("frameGeneration", first)
            }
        } else if (mode === "target") {
            const options = frameGenerationTargetOptions().filter(option => option.enabled)
            let target = frameGenerationTargetFps()
            if (!options.some(option => option.value === target)) {
                target = options.length ? options[0].value : 0
                writeFrameGenerationSetting("frameGenerationTargetFps", target)
            }
            storeFrameGenerationLevel(frameGenerationTargetFactor(target))
        } else {
            storeFrameGenerationLevel(frameGenerationDisplayFactor())
        }
        writeFrameGenerationSetting("frameGenerationMode", mode)
    }
    function selectFrameGenerationLevel(value) {
        const known = frameGenerationLevels.indexOf(String(value)) >= 0
        writeFrameGenerationSetting("frameGeneration", known ? String(value) : "off")
        writeFrameGenerationSetting("frameGenerationMode", known ? "multiplier" : "off")
    }
    function selectFrameGenerationTarget(value) {
        const target = Number(value) > 0 ? Number(value) : 0
        writeFrameGenerationSetting("frameGenerationTargetFps", target)
        storeFrameGenerationLevel(frameGenerationTargetFactor(target))
        writeFrameGenerationSetting("frameGenerationMode", target > 0 ? "target" : "off")
    }
    function selectFrameGenerationDisplayHz(value) {
        writeFrameGenerationSetting("frameGenerationDisplayHz", Number(value) > 0 ? Number(value) : 0)
    }
    // In-stream quick toggle (Ctrl+G menu). Off keeps the configured mode, level and
    // target in memory so On restores them; after a restart On takes the first level that fits.
    property string frameGenerationResumeMode: ""
    property string frameGenerationResumeLevel: ""
    property int frameGenerationResumeTarget: 0
    function toggleFrameGeneration() {
        if (frameGenerationMode() !== "off") {
            frameGenerationResumeMode = frameGenerationMode()
            frameGenerationResumeLevel = String(settings.frameGeneration || "off")
            frameGenerationResumeTarget = frameGenerationTargetFps()
            selectFrameGenerationMode("off")
            return
        }
        if (!frameGenerationUnlocked()) return
        const mode = frameGenerationResumeMode || (frameGenerationResumeTarget > 0 ? "target" : "multiplier")
        const level = frameGenerationLevelFactor(frameGenerationResumeLevel)
        if (mode === "target" && frameGenerationResumeTarget > 0)
            selectFrameGenerationTarget(frameGenerationResumeTarget)
        else if (mode === "multiplier" && level >= 2 && frameGenerationAllowed(level))
            selectFrameGenerationLevel(frameGenerationResumeLevel)
        else
            selectFrameGenerationMode(mode === "target" ? "multiplier" : mode)
    }
    // The in-stream tile only appears while frame generation is in use: on, or switched
    // off from the stream in this session so it can be switched back on.
    function frameGenerationQuickVisible() {
        return frameGenerationUnlocked() && (frameGenerationMode() !== "off" || frameGenerationResumeMode !== "")
    }
    // One line for the in-stream menu tile.
    function frameGenerationQuickText() {
        if (!frameGenerationUnlocked()) return qsTr("Run the capability test in Settings first")
        if (frameGenerationMode() === "off") return qsTr("Off")
        const factor = frameGenerationFactor()
        if (factor < 2) return qsTr("On · no level fits this frame rate")
        return qsTr("%1× · %2 FPS").arg(factor).arg(frameGenerationSourceFps() * factor)
    }
    // "2× 0.8 ms · 3× 1.1 ms · 4× 1.4 ms": p95 GPU time per source frame at each measured level.
    function frameGenerationCostText(profile) {
        const costs = {2: profile.p95Ms, 3: profile.p95Ms3, 4: profile.p95Ms4}
        const parts = []
        for (const factor of [2, 3, 4]) {
            const cost = Number(costs[factor] || 0)
            if (cost > 0) parts.push(qsTr("%1× %2 ms").arg(factor).arg(cost.toFixed(1)))
        }
        return parts.join(" · ")
    }
    // Status of the capability test for the current preset (its own settings row).
    function frameGenerationTestDescription() {
        const resolution = frameGenerationResolution().text.replace("x", "×")
        const source = frameGenerationSourceFps()
        if (FrameGenerationBenchmark.running)
            return qsTr("Testing frame generation at %1 and %2 FPS…").arg(resolution).arg(source)
        if (frameGenerationTestError !== "")
            return qsTr("Capability test failed: %1").arg(frameGenerationTestError)
        const profile = frameGenerationProfile()
        if (!profile)
            return qsTr("Not tested for %1 on this GPU. Run the test to unlock frame generation.").arg(resolution)
        const verdict = frameGenerationVerdict()
        const display = Math.round(frameGenerationDisplayHz())
        const p95 = Number(profile.p95Ms || 0).toFixed(1)
        const allowed = verdict.allowed || []
        if (allowed.length)
            return qsTr("Tested on %1 (%2 per source frame): up to %3× at %4 FPS on this %5 Hz display.")
                .arg(profile.gpu || frameGenerationGpu).arg(frameGenerationCostText(profile)).arg(Math.max.apply(null, allowed)).arg(source).arg(display)
        // Nothing fits: name the real reason for 2x, the cheapest level.
        switch (String((verdict.reasons || {})["2"] || "")) {
        case "display":
            return display > 0
                ? qsTr("Your %1 Hz display cannot show %2 FPS (2× of the %3 FPS stream). Lower the stream frame rate or move OpenNOW to a faster display.").arg(display).arg(source * 2).arg(source)
                : qsTr("Display refresh unknown. Set the override under Advanced.")
        case "gpu":
            return qsTr("Too slow on this GPU: %1 ms per source frame at 2×, budget %2 ms at %3 FPS.")
                .arg(p95).arg(Number(verdict.budgetMs || 0).toFixed(1)).arg(source)
        default:
            return qsTr("Tested on %1 (%2 per source frame), but no level fits %3 FPS right now.")
                .arg(profile.gpu || frameGenerationGpu).arg(frameGenerationCostText(profile)).arg(source)
        }
    }
    // Shown wherever frame generation is offered while FG-Legacy is the engine.
    function frameGenerationEngineNotice() {
        return qsTr("Experimental: the current engine was never designed for games, so expect strong artifacts on thin edges and fast motion. The upcoming FlyHigh model fixes this.")
    }
    // Description of the mode control: what will run for the selected mode.
    function frameGenerationDescription() {
        if (!frameGenerationUnlocked())
            return qsTr("Unavailable until the capability test passes for the selected resolution and frame rate.")
        const factor = frameGenerationFactor(), source = frameGenerationSourceFps(), display = frameGenerationDisplayHz()
        switch (frameGenerationMode()) {
        case "multiplier":
            return factor >= 2 ? qsTr("Fixed multiplier: %1× → %2 FPS.").arg(factor).arg(source * factor)
                               : qsTr("Pick a multiplier below.")
        case "target":
            return factor >= 2 ? qsTr("Target %1 FPS → %2× of the %3 FPS stream.").arg(frameGenerationTargetFps()).arg(factor).arg(source)
                               : qsTr("Pick an output rate below.")
        case "display":
            if (factor >= 2)
                return qsTr("Follows the display: %1 Hz → %2× (%3 FPS).").arg(Math.round(display)).arg(factor).arg(source * factor)
            return display > 0
                ? qsTr("The %1 Hz display cannot show 2× the %2 FPS stream.").arg(Math.round(display)).arg(source)
                : qsTr("Display refresh unknown. Set the override under Advanced.")
        default:
            return qsTr("Off. The stream is shown at its own %1 FPS.").arg(source)
        }
    }
    function frameGenerationOutputDescription() { return frameGenerationDescription() }
    function frameGenerationMultiplierDescription() {
        return qsTr("Displayed frames per source frame. Adds latency and artifacts.")
    }
    function frameGenerationTargetDescription() {
        return frameGenerationTargetOptions().length
            ? qsTr("Output rate for the %1 FPS stream; the multiplier follows.").arg(frameGenerationSourceFps())
            : qsTr("No output rate between 2× and 4× of the stream fits this display.")
    }
    function frameGenerationDisplayDescription() {
        const detected = Math.round(Number(detectedDisplayHz || 0))
        const override = Number(settings.frameGenerationDisplayHz || 0)
        if (override > 0)
            return qsTr("Override: %1 Hz (detected %2 Hz). Auto uses the display OpenNOW is on.").arg(override).arg(detected)
        return detected > 0
            ? qsTr("Auto: %1 Hz detected on the current display. Only override this when detection is wrong.").arg(detected)
            : qsTr("Display refresh could not be detected. Choose it so the pacer can schedule frames.")
    }
    function runFrameGenerationTest() {
        if (FrameGenerationBenchmark.running) return
        ensureFrameGenerationGpu()
        frameGenerationTestError = ""
        const resolution = frameGenerationResolution()
        if (!FrameGenerationBenchmark.start({engine: "motion", width: resolution.width, height: resolution.height, factor: 2, seconds: 3}))
            frameGenerationTestError = qsTr("already running")
    }
    property Connections frameGenerationBenchmarkConnections: Connections {
        target: FrameGenerationBenchmark
        function onFinished(result) {
            if (!result || result.ok !== true) {
                root.frameGenerationTestError = String((result && result.error) || "unknown")
                return
            }
            if (result.gpu) root.frameGenerationGpu = String(result.gpu)
            const source = root.frameGenerationSourceFps()
            const display = root.frameGenerationDisplayHz()
            const verdict = FrameGenerationBenchmark.evaluate(result, source, display)
            const profiles = Object.assign({}, root.settings.frameGenerationProfiles || {})
            profiles[root.frameGenerationProfileKey()] = {
                engine: "motion", gpu: String(result.gpu || root.frameGenerationGpu),
                width: result.width, height: result.height, fps: source, refreshHz: display,
                p50Ms: result.p50Ms, p95Ms: result.p95Ms, maxMs: result.maxMs, frames: result.frames,
                p95Ms3: Number(result.p95Ms3 || 0), p95Ms4: Number(result.p95Ms4 || 0),
                gpuTimed: result.gpuTimed === true, engineMaxFactor: Number(result.engineMaxFactor || 2),
                allowed: verdict.allowed, testedAt: result.testedAt, version: 1
            }
            root.writeFrameGenerationSetting("frameGenerationProfiles", profiles)
            const factor = root.frameGenerationFactor()
            if (factor >= 2 && verdict.allowed.indexOf(factor) < 0) {
                root.writeFrameGenerationSetting("frameGenerationTargetFps", 0)
                root.writeFrameGenerationSetting("frameGeneration", "off")
            }
        }
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

    function setSetting(key, value, source) {
        if (key === "launchInConsoleMode")
            return requestConsoleSurface(Boolean(value))
        if (!ready) {
            if (!ownsConfirmedSetting(key) && Object.prototype.hasOwnProperty.call(confirmedSettings, key))
                reconcileSetting(key, Object.keys(settingValues(key, value)).filter(changedKey => changedKey !== key))
            errorReported(qsTr("The OpenNOW core is not ready"))
            return ""
        }
        const id = beginSettingWrite(key, value, null, source)
        if (id !== "" && !ownsConfirmedSetting(key) && key !== "gameCollections")
            applySetting(key, value)
        else if (id === "" && !ownsConfirmedSetting(key)
                 && Object.prototype.hasOwnProperty.call(confirmedSettings, key))
            reconcileSetting(key, Object.keys(settingValues(key, value)).filter(changedKey => changedKey !== key))
        return id
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

    function settingValues(key, value) {
        const updated = {[key]:value}
        if (key === "themePack") {
            updated.appTheme = value === "bone" || value === "cobalt" ? "light" : "dark"
            updated.themeAccentOverride = false
        } else if (key === "appAccentColor") {
            updated.themeAccentOverride = true
        }
        return updated
    }

    function applySetting(key, value) {
        settings = Object.assign({}, settings, settingValues(key, value))
        if (["nativeStreamerExecutablePath", "nativeVideoBackend", "decoderPreference"].indexOf(key) >= 0)
            Qt.callLater(root.refreshStreamerDetection)
        accessibilityAnnounced(qsTr("%1 updated").arg(String(key).split(/(?=[A-Z])/).join(" ")))
        if (key === "reducedMotion")
            appController.reducedMotion = Boolean(value)
        if (key === "appLanguage")
            i18n.setLocale(String(value || "system"))
        if (key === "discordRichPresence")
            syncDiscordPresence()
    }

    function acceptSettings(result) {
        const confirmed = Object.assign({}, result.settings)
        const displayed = Object.assign({}, result.settings)
        for (const key of Object.keys(settingWrites)) {
            confirmed[key] = confirmedSettings[key]
            displayed[key] = settings[key]
            for (const coupledKey of settingWrites[key].coupledKeys) {
                confirmed[coupledKey] = confirmedSettings[coupledKey]
                displayed[coupledKey] = settings[coupledKey]
            }
        }
        if (consoleSurfaceRequestId !== "") {
            confirmed.launchInConsoleMode = consoleSurfaceConfirmedValue
            displayed.launchInConsoleMode = consoleSurfaceDesiredValue
        } else {
            root.consoleSurfaceConfirmedValue = Boolean(result.settings.launchInConsoleMode)
            root.consoleSurfaceDesiredValue = root.consoleSurfaceConfirmedValue
            root.consoleSurfaceInitialized = true
        }
        root.confirmedSettings = confirmed
        root.settings = displayed
        root.keyboardLayouts = result.keyboardLayouts || []
        appController.reducedMotion = Boolean(displayed.reducedMotion)
        i18n.setLocale(String(displayed.appLanguage || "system"))
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
        const write = root.settingWrites[payload.key]
        if (write) return
        confirmedSettings = Object.assign({}, confirmedSettings, payload.changes || {}, {[payload.key]:payload.value})
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

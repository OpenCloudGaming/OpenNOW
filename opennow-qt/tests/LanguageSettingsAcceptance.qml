import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property var screen: null
    property var picker: null
    property var owner: ShellStore.settingsOwnerState
    property int settingAccountRefreshes: 0
    property Component consoleSettings: Component { SettingsScreen { visible: false } }
    property QtObject hdrOutput: QtObject {
        property bool supported: false
        property int outputMode: 0
        property bool chromeRequired: false
        property string status: "Synthetic display capability"
    }
    property QtObject client: QtObject {
        property int serial: 0
        property var calls: []
        function request(method, params, timeout) {
            const id = "language-fixture-" + (++serial)
            calls = calls.concat([{id:id, method:method, params:params, timeout:timeout}])
            return id
        }
        function cancel(id) {
            fixture.check(owner.languageRequestId !== id && owner.colorRequestId !== id,
                "ownership must be cleared before synchronous cancellation")
            fixture.check(owner.acceptFailure(id, "cancelled"), "synchronous cancellation is consumed")
        }
    }

    function check(ok, message) { if (!ok) throw new Error("Language settings: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function reply(languages, status, extra) {
        check(owner.languageRequestId !== "", "language request exists")
        owner.acceptResponse(owner.languageRequestId, Object.assign({languages:languages,
            status:status, source:"overallGfnSupportedLanguages", scopeGeneration:owner.scopeGeneration,
            fetchedAt:Date.now(), expiresAt:Date.now() + 1200000}, extra || {}))
    }
    function verifyOrdinarySettingWrites() {
        const saved = owner.settings
        owner.acceptSettings({settings:Object.assign({}, owner.settings, {windowWidth:900}), keyboardLayouts:owner.keyboardLayouts})
        const before = client.calls.length
        for (const width of [1000, 1010, 1020, 1030]) {
            owner.setSetting("windowWidth", width)
        }
        check(owner.settings.windowWidth === 1030, "ordinary setters publish the latest optimistic value")
        check(client.calls.length === before + 1, "ordinary writes have only one request in flight per key")
        check(!owner.ownsConfirmedSetting("windowWidth"), "ordinary controls retain their optimistic policy")
        const first = client.calls[before].id
        owner.acceptSettingsChange({key:"windowWidth", value:1000})
        check(owner.settings.windowWidth === 1030, "an older event cannot undo the latest optimistic intent")
        check(!owner.acceptResponse(first, {key:"windowWidth", value:1000}),
            "ordinary completions remain available to their callers")
        check(owner.settings.windowWidth === 1030, "an older response cannot undo the latest optimistic intent")
        check(client.calls.length === before + 2 && client.calls[before + 1].params.value === 1030,
            "ordinary writes coalesce to the last intent")
        const latest = client.calls[before + 1].id
        owner.acceptSettingsChange({key:"windowWidth", value:1000})
        check(owner.settings.windowWidth === 1030, "a delayed prior event cannot replace the in-flight latest intent")
        owner.acceptSettingsChange({key:"windowWidth", value:1030})
        owner.acceptResponse(latest, {key:"windowWidth", value:1030})
        check(!owner.acceptResponse(first, {key:"windowWidth", value:1000}), "late ordinary replies are unowned")
        check(!owner.acceptFailure(first, "Late old failure") && owner.settings.windowWidth === 1030,
            "late ordinary failures cannot roll back the latest value")
        owner.setSetting("windowWidth", 1040)
        check(!owner.acceptFailure(owner.settingWrites.windowWidth.id, "Synthetic ordinary write failure"),
            "ordinary failures remain available to their callers")
        check(owner.settings.windowWidth === 1030, "failed ordinary writes preserve the confirmed value")
        owner.setSetting("windowWidth", 1050)
        const failed = owner.settingWrites.windowWidth.id
        owner.setSetting("windowWidth", 1060)
        owner.acceptFailure(failed, "Synthetic superseded write failure")
        const retry = owner.settingWrites.windowWidth.id
        check(retry !== failed && client.calls[client.calls.length - 1].params.value === 1060,
            "a rejected older write still dispatches the latest intent")
        owner.acceptSettingsChange({key:"windowWidth", value:1060})
        owner.acceptResponse(retry, {key:"windowWidth", value:1060})
        owner.setSetting("windowWidth", 1065)
        const successfulHead = owner.settingWrites.windowWidth.id
        owner.setSetting("windowWidth", 1069)
        owner.acceptSettingsChange({key:"windowWidth", value:1065})
        owner.acceptResponse(successfulHead, {key:"windowWidth", value:1065})
        owner.acceptFailure(owner.settingWrites.windowWidth.id, "Synthetic failed tail")
        check(owner.settings.windowWidth === 1065, "a rejected tail restores the successful head rather than the unsaved intent")
        owner.setSetting("windowWidth", 1060)
        owner.acceptResponse(owner.settingWrites.windowWidth.id, {key:"windowWidth",value:1060})
        owner.setSetting("windowWidth", 1070)
        const disconnected = owner.settingWrites.windowWidth.id
        owner.setSetting("windowWidth", 1080)
        const beforeDisconnect = client.calls.length
        owner.acceptSettings({settings:Object.assign({}, owner.confirmedSettings, {windowWidth:900, windowHeight:901}),
            keyboardLayouts:owner.keyboardLayouts})
        check(owner.settings.windowWidth === 1080 && owner.confirmedSettings.windowWidth === 1060
            && owner.settings.windowHeight === 901, "a settings snapshot preserves pending intent and its confirmed rollback value")
        owner.ready = false
        check(!owner.settingWrites.windowWidth, "readiness loss releases pending writes before failure callbacks")
        owner.acceptFailure(disconnected, "Synthetic core disconnect")
        owner.ready = true
        check(client.calls.length === beforeDisconnect && !owner.settingWrites.windowWidth,
            "disconnect drops queued writes instead of replaying them after reconnect")
        check(!owner.acceptResponse(disconnected, {key:"windowWidth", value:1070})
            && owner.settings.windowWidth === 1060, "a disconnected reply cannot restore an older value")

        const provider = owner.providerIdpId
        owner.providerIdpId = "settings-provider-a"
        owner.setSetting("region", "region-a")
        const region = owner.settingWrites.region.id
        check(client.calls[client.calls.length - 1].params.providerIdpId === "settings-provider-a",
            "region writes retain their provider")
        owner.setSetting("region", "region-a-newer")
        owner.providerIdpId = "settings-provider-b"
        const beforeSwitch = client.calls.length
        owner.acceptSettingsChange({key:"region", value:"region-a", changes:{
            regionProviderIdpId:"settings-provider-a", providerRegions:{"settings-provider-a":"region-a"}}})
        owner.acceptResponse(region, {key:"region", value:"region-a", changes:{
            regionProviderIdpId:"settings-provider-a", providerRegions:{"settings-provider-a":"region-a"}}})
        check(client.calls.length === beforeSwitch && owner.selectedRegion === "",
            "a provider switch never replays the old provider's queued region")
        owner.setSetting("region", "region-b")
        const regionB = owner.settingWrites.region.id
        owner.setSetting("region", "region-b-newer")
        owner.acceptFailure(regionB, "Synthetic rejected region")
        check(client.calls[client.calls.length - 1].params.providerIdpId === "settings-provider-b"
            && client.calls[client.calls.length - 1].params.value === "region-b-newer",
            "queued region writes keep the current provider context")
        owner.acceptSettingsChange({key:"region", value:"region-b-newer", changes:{
            regionProviderIdpId:"settings-provider-b", providerRegions:{"settings-provider-b":"region-b-newer"}}})
        owner.acceptResponse(owner.settingWrites.region.id, {key:"region", value:"region-b-newer", changes:{
            regionProviderIdpId:"settings-provider-b", providerRegions:{"settings-provider-b":"region-b-newer"}}})
        check(owner.selectedRegion === "region-b-newer", "confirmed region updates retain coupled provider metadata")
        owner.setSetting("region", "region-b")
        const oldScope = owner.settingWrites.region.id
        owner.setSetting("region", "region-b-latest")
        owner.scopeGeneration += 1
        const beforeAccountChange = client.calls.length
        owner.acceptFailure(oldScope, "Synthetic account change")
        check(client.calls.length === beforeAccountChange && !owner.settingWrites.region,
            "changing accounts within a provider discards the old account's queued region")
        owner.providerIdpId = provider

        owner.setSetting("colorQuality", "10bit_420")
        owner.acceptResponse(owner.settingWrites.colorQuality.id,
            {key:"colorQuality", value:"10bit_420", changes:{codec:"auto", fallbackCodec:"h265"}})
        check(owner.settings.colorQuality === "10bit_420" && owner.settings.codec === "auto"
            && owner.settings.fallbackCodec === "h265", "confirmed writes retain coupled codec repairs")
        check(!owner.ownsConfirmedSetting("launchInConsoleMode"), "console mode retains its separate optimistic path")

        const refresh = owner.refreshAccountServices
        owner.refreshAccountServices = function() {
            fixture.settingAccountRefreshes += 1
            owner.refreshAccountServices = refresh
        }
        owner.acceptSettingsChange({key:"identifyAsSteamDeck", value:false})
        owner.setSetting("identifyAsSteamDeck", true)
        const identity = owner.settingWrites.identifyAsSteamDeck.id
        owner.setSetting("identifyAsSteamDeck", false)
        check(settingAccountRefreshes === 0, "device identity does not refresh entitlements before saving")
        owner.acceptSettingsChange({key:"identifyAsSteamDeck", value:true})
        check(settingAccountRefreshes === 0, "owned identity events wait for acknowledgement")
        owner.acceptResponse(identity, {key:"identifyAsSteamDeck", value:true})
        owner.acceptFailure(owner.settingWrites.identifyAsSteamDeck.id, "Synthetic rejected identity tail")
        check(owner.settings.identifyAsSteamDeck === true,
            "a failed identity tail restores the persisted head before refreshing entitlements")
        owner.setSetting("gameCollections", [{id:"settings-test", name:"Settings test", gameIds:[]}])
        const collection = owner.settingWrites.gameCollections.id
        check(!owner.acceptResponse(collection, {key:"gameCollections", value:[]})
            && !owner.settingWrites.gameCollections, "collection saves release the queue without consuming the caller's response")
        owner.setSetting("gameCollections", [])
        check(!owner.acceptFailure(owner.settingWrites.gameCollections.id, "Synthetic collection failure")
            && !owner.settingWrites.gameCollections, "collection errors release the queue without consuming the caller's failure")
        owner.setSetting("gameCollections", [])
        const disconnectedCollection = owner.settingWrites.gameCollections.id
        owner.ready = false
        owner.ready = true
        const reconnectedCollection = owner.setSetting("gameCollections", [])
        check(reconnectedCollection !== disconnectedCollection,
            "collection saves after reconnect do not wait for a disconnected request")
        owner.acceptResponse(reconnectedCollection, {key:"gameCollections", value:[]})
        owner.acceptSettings({settings:saved, keyboardLayouts:owner.keyboardLayouts})
    }
    function verifyRapidSettingCallers(parent) {
        const saved = owner.settings
        const catalog = ShellStore.catalogOwnerState
        owner.acceptSettings({settings:Object.assign({}, saved, {favoriteGameIds:[], hiddenGameIds:[],
            homeTileSizes:{}, reducedMotion:false}), keyboardLayouts:owner.keyboardLayouts})
        for (const game of [{id:"rapid-a"}, {id:"rapid-b"}]) {
            catalog.addToHome(game)
            catalog.toggleHidden(game)
            catalog.setHomeTileSize(game, "wide")
        }
        check(JSON.stringify(owner.settings.favoriteGameIds) === '["rapid-a","rapid-b"]'
            && JSON.stringify(owner.settings.hiddenGameIds) === '["rapid-a","rapid-b"]',
            "real catalog actions derive each new replacement from the latest optimistic arrays")
        check(owner.settings.homeTileSizes["rapid-a"] === "wide" && owner.settings.homeTileSizes["rapid-b"] === "wide",
            "real tile edits preserve independently edited map entries")
        for (const key of ["favoriteGameIds", "hiddenGameIds", "homeTileSizes"]) {
            while (owner.settingWrites[key]) {
                const write = owner.settingWrites[key]
                owner.acceptSettingsChange({key:key,value:write.value})
                owner.acceptResponse(write.id, {key:key,value:write.value})
            }
        }
        check(owner.settings.favoriteGameIds.length === 2 && owner.settings.hiddenGameIds.length === 2,
            "serialized catalog writes persist both independent edits")
        const consolePage = consoleSettings.createObject(parent)
        check(consolePage, "production console settings can be created")
        consolePage.activate({toggle:true,key:"reducedMotion"})
        const first = owner.settingWrites.reducedMotion.id
        consolePage.activate({toggle:true,key:"reducedMotion"})
        check(owner.settings.reducedMotion === false && owner.settingWrites.reducedMotion.next === false,
            "two production console toggles return to the original value before persistence")
        owner.acceptResponse(first, {key:"reducedMotion",value:true})
        owner.acceptSettingsChange({key:"reducedMotion",value:true})
        check(owner.settings.reducedMotion === false, "an older delayed event cannot undo the second console toggle")
        owner.acceptResponse(owner.settingWrites.reducedMotion.id, {key:"reducedMotion",value:false})
        consolePage.destroy()
        owner.acceptSettings({settings:saved, keyboardLayouts:owner.keyboardLayouts})
    }
    function verifyCoupledSettingWrites() {
        const saved = owner.settings
        for (const disconnect of [false, true]) {
            for (const acknowledged of [false, true]) {
                owner.acceptSettings({settings:Object.assign({}, saved, {themePack:"nocturne", appTheme:"dark",
                    themeAccentOverride:false}), keyboardLayouts:owner.keyboardLayouts})
                owner.setSetting("themePack", "bone")
                const theme = owner.settingWrites.themePack.id
                owner.setSetting("appTheme", "auto")
                const appearance = owner.settingWrites.appTheme.id
                if (acknowledged) owner.acceptResponse(appearance, {key:"appTheme", value:"auto"})
                if (disconnect) owner.ready = false
                else owner.acceptFailure(theme, "Synthetic rejected theme")
                check(owner.settings.themePack === "nocturne"
                    && owner.settings.appTheme === (acknowledged || !disconnect ? "auto" : "dark"),
                    "theme rollback uses current confirmed values and preserves surviving sibling intent")
                if (disconnect) owner.ready = true
                else if (!acknowledged) {
                    owner.acceptFailure(appearance, "Synthetic rejected appearance")
                    check(owner.settings.appTheme === "dark", "a rejected sibling restores its own confirmed value")
                }
            }
        }
        owner.acceptSettings({settings:Object.assign({}, saved, {themePack:"nocturne", appTheme:"auto",
            themeAccentOverride:true}), keyboardLayouts:owner.keyboardLayouts})
        owner.setSetting("themePack", "nocturne")
        check(owner.settings.appTheme === "dark" && owner.settings.themeAccentOverride === false,
            "reselecting the current theme resets its appearance and accent overrides optimistically")
        owner.acceptFailure(owner.settingWrites.themePack.id, "Synthetic rejected same-theme reset")
        check(owner.settings.appTheme === "auto" && owner.settings.themeAccentOverride === true,
            "a rejected same-theme reset restores confirmed overrides")
        owner.setSetting("appAccentColor", "blue")
        const accent = owner.settingWrites.appAccentColor.id
        owner.setSetting("themePack", "bone")
        owner.acceptFailure(owner.settingWrites.themePack.id, "Synthetic rejected later theme")
        check(owner.settings.themeAccentOverride === true,
            "rollback restores an earlier pending accent write's coupled intent")
        owner.acceptFailure(accent, "Synthetic rejected accent")
        owner.acceptSettings({settings:saved, keyboardLayouts:owner.keyboardLayouts})
    }
    function run(parent) {
        owner.settingsActive = false
        owner.coreClient = client
        owner.ready = true
        owner.scopeGeneration = 12
        owner.settings = Object.assign({}, owner.settings, {appLanguage:"system", gameLanguage:"es_419",
            keyboardLayout:"ja-JP", colorQuality:"8bit_444", codec:"h265"})
        owner.keyboardLayouts = [{value:"en-US",label:"English (US)",aliases:[]},
            {value:"ja-106",label:"Japanese 106",aliases:["ja-JP","Japanese106"]},
            {value:"es-ES_tradnl",label:"Spanish (traditional)",aliases:["es-ES"]}]
        check(client.calls.length === 0, "metadata is lazy")
        owner.ensureGameLanguages()
        const first = owner.languageRequestId
        owner.ensureGameLanguages()
        check(client.calls.length === 1 && client.calls[0].timeout === 15000, "single flight and bounded deadline")
        check(JSON.stringify(client.calls[0].params) === '{"refresh":false}', "exact metadata RPC parameters")
        owner.scopeGeneration = 13
        check(!owner.acceptResponse(first, {languages:["de_DE"], status:"success", scopeGeneration:12}), "late reply rejected")
        check(owner.gameLanguageItems[0].value === "es_419", "saved offline ID retained exactly")
        owner.ensureGameLanguages()
        reply(["en_US","es_419","zh_Hant_TW"], "success", {cacheHit:true})
        check(owner.languageStatusText.indexOf("cached") >= 0, "cache provenance shown")
        const calls = client.calls.length
        owner.ensureGameLanguages()
        check(client.calls.length === calls, "fresh settings revisit does not refetch")
        owner.ensureGameLanguages(true)
        reply(["en_US","es_419"], "stale", {error:{message:"Synthetic network failure"}})
        check(owner.languageStatusText.indexOf("stale") >= 0, "stale provenance shown")
        owner.ensureGameLanguages(true)
        owner.languageDeadline.triggered()
        check(owner.languageState === "stale" && owner.languageRequestId === "", "timeout retains metadata and clears ownership")
        owner.settings = Object.assign({}, owner.settings, {sessionProxyEnabled:true, sessionProxyUrl:"http://proxy.example.invalid:8080"})
        check(owner.languageState === "idle" && !owner.languageResult.languages, "proxy invalidates metadata")
        owner.ensureGameLanguages()
        reply(["en_US"], "success", {scopeGeneration:11})
        check(owner.languageState === "error", "wrong server scope cannot be accepted")
        owner.ensureGameLanguages(true)
        owner.acceptFailure(owner.languageRequestId, "Synthetic offline failure")
        check(owner.languageState === "error" && owner.settings.gameLanguage === "es_419", "offline failure preserves preference")

        owner.setSetting("gameLanguage", "pt_BR")
        const rejected = owner.settingWrites.gameLanguage.id
        check(owner.settings.gameLanguage === "es_419", "no false optimistic selection")
        owner.setSetting("gameLanguage", "zh_Hant_TW")
        owner.acceptFailure(rejected, "Synthetic rejected write")
        const newer = owner.settingWrites.gameLanguage.id
        check(newer !== rejected, "rapid edits are serialized")
        owner.acceptSettingsChange({key:"gameLanguage",value:"zh_Hant_TW"})
        owner.acceptResponse(newer, {key:"gameLanguage",value:"zh_Hant_TW"})
        owner.acceptFailure(rejected, "Late old rejection")
        check(owner.settings.gameLanguage === "zh_Hant_TW", "old failure cannot undo new success")
        check(owner.settings.appLanguage === "system" && owner.settings.keyboardLayout === "ja-JP", "three persistence keys stay independent")
        check(owner.interfaceLanguageItems[0].value === "system"
            && !owner.interfaceLanguageItems.some(item => item.value === "es_419"), "bundled interface choices are independent")
        for (const failSecond of [false, true]) {
            owner.setSetting("gameLanguage", "zh_Hant_TW")
            const firstWrite = owner.settingWrites.gameLanguage.id
            owner.setSetting("gameLanguage", "fr_FR")
            const requestCount = client.calls.length
            owner.acceptSettingsChange({key:"gameLanguage",value:"zh_Hant_TW"})
            check(client.calls.length === requestCount, "the first event cannot start the queued write")
            owner.acceptResponse(firstWrite, {key:"gameLanguage",value:"zh_Hant_TW"})
            const secondWrite = owner.settingWrites.gameLanguage.id
            check(secondWrite !== firstWrite && client.calls.length === requestCount + 1,
                "the queued write starts only after the first event and response")
            if (failSecond) owner.acceptFailure(secondWrite, "Synthetic second write failure")
            else {
                owner.acceptSettingsChange({key:"gameLanguage",value:"fr_FR"})
                check(owner.settings.gameLanguage === "zh_Hant_TW", "the second event waits for its acknowledgement")
                owner.acceptResponse(secondWrite, {key:"gameLanguage",value:"fr_FR"})
            }
            owner.acceptFailure(firstWrite, "Late old failure")
            check(owner.settings.gameLanguage === (failSecond ? "zh_Hant_TW" : "fr_FR"),
                "queued success or failure keeps the last confirmed value")
        }
        owner.acceptSettingsChange({key:"gameLanguage",value:"it_IT"})
        check(owner.settings.gameLanguage === "it_IT", "legitimate unowned settings events still apply")
        owner.acceptSettingsChange({key:"gameLanguage",value:"zh_Hant_TW"})
        verifyOrdinarySettingWrites()
        verifyRapidSettingCallers(parent)
        verifyCoupledSettingWrites()
        const beforeCoupled = owner.settings
        owner.acceptSettingsChange({key:"themePack",value:"bone",changes:{appTheme:"light",themeAccentOverride:false}})
        check(owner.settings.themePack === "bone" && owner.settings.appTheme === "light"
            && owner.settings.themeAccentOverride === false, "coupled settings events remain atomic")
        owner.settings = beforeCoupled
        owner.setSetting("keyboardLayout", "es-ES_tradnl")
        owner.acceptFailure(owner.settingWrites.keyboardLayout.id, "Synthetic rejected layout")
        check(owner.settings.keyboardLayout === "ja-JP", "rejected layout is not shown as saved")
        check(owner.keyboardLayoutItems[0].value === "ja-JP"
            && owner.keyboardLayoutItems[0].detail.indexOf("ja-106") >= 0, "legacy layout shown truthfully")
        owner.ready = false
        check(owner.settings.gameLanguage === "zh_Hant_TW" && owner.languageState === "idle", "logout/readiness loss preserves saved values")
        owner.ready = true
        check(owner.colorQualityItems.length === 4 && owner.colorQualityItems.every(item => item.disabled), "unknown capability is not support")
        check(owner.colorQualityItems.filter(item => item.value.endsWith("_444"))
            .every(item => item.detail === qsTr("Coming soon")), "unconfirmed 4:4:4 profiles are marked coming soon")
        owner.colorDescriptors = ["8bit_420", "8bit_444", "10bit_420", "10bit_444"]
            .map(value => ({value:value, disabled:false, reason:"Supported"}))
        check(owner.colorQualityItems.filter(item => item.value.endsWith("_444"))
            .every(item => item.detail === qsTr("Coming soon")), "4:4:4 never advertises support from capability descriptors")
        check(owner.colorQualityItems.filter(item => item.value.endsWith("_420"))
            .every(item => item.detail === (owner.colorQualityGate(item.value) || "Supported")), "4:2:0 retains its existing availability status")
        check(owner.colorQualityItems.every(item => item.disabled === (owner.colorQualityGate(item.value) !== "")),
            "coming soon labels preserve the existing selection gates")
        owner.colorDescriptors = []
        owner.colorRequestId = "old-colors"
        owner.settings = Object.assign({}, owner.settings, {codec:"h264"})
        check(owner.colorRequestId === "" && !owner.acceptResponse("old-colors", {colorQualities:[{value:"8bit_444",disabled:false}]}),
            "codec change cancels old capability choices")
        check(owner.settings.colorQuality === "8bit_444", "codec changes do not coerce saved color")
        owner.colorRequestId = "colors-fixture"
        const colorChoices = [
            {value:"8bit_420",disabled:false}, {value:"8bit_444",disabled:true,reason:"Synthetic backend does not support 4:4:4"},
            {value:"10bit_420",disabled:false}, {value:"10bit_444",disabled:true,reason:"Synthetic backend does not support 4:4:4"}]
        owner.acceptResponse("colors-fixture", {colorQualities:colorChoices})
        check(owner.settings.colorQuality === "8bit_444" && owner.colorQualityItems[1].disabled, "unsupported saved color remains visible")

        owner.settings = Object.assign({}, owner.settings, {gameLanguage:"future_001", codec:"h265", sessionProxyEnabled:false,
            desktopUiScale:Qt.application.arguments.indexOf("--smoke-light-theme") >= 0 ? 1.25 : 1,
            sessionProxyUrl:"", appTheme:Qt.application.arguments.indexOf("--smoke-light-theme") >= 0 ? "light" : "dark"})
        ShellStore.nativeRuntimeReady = true
        ShellStore.nativeRuntimeCapabilities = {protocolVersion:7,
            videoBackends:[{backend:"vaapi",available:true,codecs:[{codec:"h265",available:true,
                colorQualities:["8bit_420","10bit_420"]}]}]}
        if (Qt.application.arguments.indexOf("--language-hdr-invalidation") >= 0) {
            owner.settingsActive = true
            let previous = ""
            for (const supported of [false, true, false]) {
                hdrOutput.supported = supported
                check(owner.nativeHdrOutputSupported === supported, "the production HdrOutput binding tracks display support")
                check(owner.colorRequestId === "", "display changes clear request ownership before cancellation")
                owner.colorRefresh.triggered()
                const request = client.calls[client.calls.length - 1]
                check(request.id === owner.colorRequestId && request.method === "settings.choices.get",
                    "a display change refetches the color descriptors")
                check(request.params.runtimeCapabilities === owner.nativeRuntimeCapabilities
                    && request.params.capabilities === undefined
                    && request.params.runtimeCapabilities.nativeHdrSupported === undefined,
                    "QML forwards native capabilities without authoring display support")
                if (previous !== "") check(!owner.acceptResponse(previous, {colorQualities:colorChoices}),
                    "late previous-display results cannot populate current choices")
                previous = request.id
            }
            owner.acceptResponse(previous, {colorQualities:colorChoices})
            owner.settingsActive = false
        }
        owner.colorDescriptors = colorChoices
        owner.ensureGameLanguages(true)
        const languages = ["en_US","en_GB","de_DE","es_419","es_ES","fr_FR","it_IT","ja_JP","ko_KR","pt_BR","zh_Hant_TW"]
        for (let index = 0; index < 100; ++index) languages.push("future_" + index)
        reply(languages, "stale", {error:{message:"Synthetic offline metadata fixture"},cacheHit:true})
        if (Qt.application.arguments.indexOf("--language-error") >= 0) {
            owner.languageResult = ({})
            owner.languageState = "error"
            owner.languageError = "Synthetic offline metadata fixture"
        }
        screen = find(parent, "desktopSettingsScreen") || find(parent, "consoleSettingsScreen")
        check(screen !== null, "production settings screen exists")
        if (screen.objectName === "desktopSettingsScreen") {
            const colors = Qt.application.arguments.indexOf("--language-colors") >= 0
            picker = find(parent, colors ? "colorQualityChoice" : "gameLanguageChoice")
            check(picker && (colors || find(parent, "keyboardLayoutChoice")), "production choices are present")
            check(JSON.stringify(picker.items) === JSON.stringify(colors ? owner.colorQualityItems : owner.gameLanguageItems), "desktop shares exact choices")
            check(JSON.stringify(screen.colorQualityItems()) === JSON.stringify(owner.colorQualityItems), "desktop shares color choices")
            picker.expanded = true
            if (Qt.application.arguments.indexOf("--language-keyboard-selection") >= 0) {
                const filter = picker.children.find(child => child.placeholderText === picker.filterPlaceholder)
                check(filter, "production filter exists")
                filter.text = "es_419"
            }
        } else {
            const list = find(screen, "consoleSettingsList")
            const colorView = Qt.application.arguments.indexOf("--language-colors") >= 0
            const index = screen.settingsModel().findIndex(item => item.key === (colorView ? "colorQuality" : "gameLanguage"))
            check(list && index >= 0, "production console row exists")
            list.currentIndex = index
            list.positionViewAtIndex(index, ListView.Center)
            const row = screen.descriptorChoice("Game language", owner.gameLanguageDescription, "gameLanguage", owner.gameLanguageItems)
            check(JSON.stringify(row.values) === JSON.stringify(owner.gameLanguageItems.map(item => item.value)), "console shares exact game IDs")
            const colors = screen.descriptorChoice("Color quality", owner.colorDescription, "colorQuality", owner.colorQualityItems)
            check(JSON.stringify(colors.disabledValues) === JSON.stringify(owner.colorQualityItems.filter(item => item.disabled).map(item => item.value)), "console shares capability decisions")
            screen.openChoices(Qt.application.arguments.indexOf("--language-colors") >= 0 ? colors : row)
            if (colorView) check(screen.dropdownPanelHeight >= 4 * 64 + 91, "all four color rows fit with their reasons")
        }
        ShellStore.lastError = ""
        return true
    }

    function verify() {
        check(settingAccountRefreshes === 1, "confirmed device identity refreshes entitlements once")
        check(picker ? !picker.expanded : !screen.dropdownOpen, "keyboard interaction closes the real production picker")
        if (Qt.application.arguments.indexOf("--language-keyboard-selection") >= 0) {
            check(owner.settingWrites.gameLanguage && owner.settingWrites.gameLanguage.value === "es_419",
                "Tab and Enter select the exact filtered language ID")
            check(owner.settings.gameLanguage === "future_001", "keyboard choice waits for persistence")
            owner.acceptResponse(owner.settingWrites.gameLanguage.id, {value:"es_419"})
            check(owner.settings.gameLanguage === "es_419", "keyboard choice confirms after persistence")
        }
        if (Qt.application.arguments.indexOf("--screenshot") >= 0
                && Qt.application.arguments.indexOf("--language-error") < 0) {
            if (picker) {
                picker.expanded = true
                if (Qt.application.arguments.indexOf("--language-colors") >= 0) Qt.callLater(() => {
                    const content = find(screen, "desktopSettingsContent")
                    content.contentY = Math.min(content.contentHeight - content.height,
                        picker.mapToItem(content.contentItem, 0, 0).y)
                })
            }
            else {
                const colors = Qt.application.arguments.indexOf("--language-colors") >= 0
                screen.openChoices(screen.descriptorChoice(colors ? "Color quality" : "Game language",
                    colors ? owner.colorDescription : owner.gameLanguageDescription,
                    colors ? "colorQuality" : "gameLanguage", colors ? owner.colorQualityItems : owner.gameLanguageItems))
            }
        }
        return true
    }
}

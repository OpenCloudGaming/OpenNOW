import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property var shell: null
    property var control: null
    property var owner: ShellStore.settingsOwnerState
    property QtObject client: QtObject {
        property int serial: 0
        property var calls: []
        function request(method, params, timeout) {
            const id = "frame-rate-fixture-" + (++serial)
            calls = calls.concat([{id:id, method:method, params:params, timeout:timeout}])
            return id
        }
        function cancel(id) {}
    }

    function check(ok, message) { if (!ok) throw new Error("Frame rate settings: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function descriptors(hardware, reason) {
        const choices = []
        for (const value of [30, 60, 90, 120, 144, 165, 240, 360]) {
            const locked = value === 360 && !hardware
            choices.push({value:value, disabled:locked, reason:locked ? reason : null})
        }
        return choices
    }
    function acceptFailure(id, message) { owner.acceptFailure(id, message) }
    function acceptResponse(id, result) { return owner.acceptResponse(id, result) }
    function capability(hardware) {
        ShellStore.nativeRuntimeReady = true
        ShellStore.nativeRuntimeCapabilities = hardware
            ? {protocolVersion:7, videoBackends:[{backend:"vaapi", available:true,
                codecs:[{codec:"h265", available:true, colorQualities:["8bit_420"]}]}]}
            : {protocolVersion:7, videoBackends:[{backend:"software", available:true,
                codecs:[{codec:"h265", available:true, colorQualities:["8bit_420"]}]}]}
    }
    function request(hardware, reason) {
        owner.colorRefresh.triggered()
        check(owner.colorRequestId !== "", "the capability request did not start")
        owner.acceptResponse(owner.colorRequestId, {colorQualities:[], frameRates:descriptors(hardware, reason)})
    }
    function entitlement(width, height, fps) {
        ShellStore.subscription = {membershipTier:"ULTIMATE", entitledResolutions:[
            {width:width, height:height, fps:fps}]}
    }
    function optionEnabled(value) {
        const option = find(shell, "settingsOption-" + value)
        check(option !== null, "the rendered control exposes " + value)
        return option.enabled
    }

    function verify() {
        const locked = Qt.application.arguments.indexOf("--frame-rate-locked") >= 0
        entitlement(1920, 1080, 360)
        owner.settings = Object.assign({}, owner.settings, {resolution:"1920x1080", fps:360, codec:"h265"})
        capability(!locked)
        request(!locked, "360 FPS needs a hardware video decoder for the selected codec. This device has none available.")
        if (locked)
            check(owner.lockedFpsValues("1920x1080").indexOf(360) >= 0, "the captured profile locks 360")
        else
            check(owner.selectableFpsValues("1920x1080").indexOf(360) >= 0,
                "the captured profile can select 360")
        if (Qt.application.arguments.indexOf("--screenshot") >= 0) {
            const content = find(shell, "desktopSettingsContent")
            if (content) {
                Qt.callLater(() => {
                    const rate = find(shell, "desktopFrameRateControl")
                    if (rate)
                        content.contentY = Math.min(content.contentHeight - content.height,
                            rate.mapToItem(content.contentItem, 0, 0).y - 24)
                })
            } else {
                const list = find(shell, "consoleSettingsList")
                const model = shell.settingsModel()
                const index = model.findIndex(item => item.key === "fps")
                if (list && index >= 0) {
                    list.currentIndex = index
                    list.positionViewAtIndex(index, ListView.Center)
                }
            }
        }
        return true
    }

    function run(parent) {
        owner.coreClient = client
        owner.ready = true
        ShellStore.nativeRuntimeReady = true
        check(owner.capabilitiesActive === true, "the production settings route activates the choices lifecycle")
        check(owner.settingsActive === true, "the production settings route activates the language lifecycle")
        owner.settings = Object.assign({}, owner.settings, {resolution:"1920x1080", fps:360, codec:"h265"})
        ShellStore.subscription = null
        check(owner.maxEntitledFps("1920x1080") === 0, "unloaded membership reports no entitlement limit")
        check(owner.lockedFpsValues("1920x1080").indexOf(360) >= 0,
            "an unconfirmed conditional tier is not offered")
        check(owner.selectableFpsValues("1920x1080").indexOf(240) >= 0,
            "base rates stay selectable without a verdict")
        const canonical = owner.canonicalFpsValues()
        check(canonical.indexOf(360) === canonical.length - 1, "360 is the canonical top rate")
        check(owner.presetFpsForResolution(1920, 1080).indexOf(360) >= 0,
            "full HD advertises the documented top tier")
        check(owner.presetFpsForResolution(1920, 1200).indexOf(360) >= 0, "full HD 16:10 advertises it")
        check(owner.presetFpsForResolution(2560, 1440).indexOf(360) < 0,
            "other resolutions never advertise it")

        entitlement(1920, 1080, 360)
        check(owner.maxEntitledFps("1920x1080") === 360, "the entitlement limit reflects the membership")

        owner.colorRequestId = "in-flight"
        check(owner.frameRateDescriptor(360) === null, "the descriptor is not known yet")
        owner.clampFpsToEntitlement()
        check(owner.settings.fps === 360, "an unknown verdict never rewrites the saved preference")
        check(owner.settingWrites.fps === undefined, "no settings write is issued for an unknown verdict")
        check(owner.resolveEntitledFps("1920x1080", 360) === 360,
            "the preference resolver leaves an unconfirmed rate alone")
        check(owner.lockedFpsValues("1920x1080").indexOf(360) >= 0,
            "the offered surface still refuses to advertise the unconfirmed rate")
        acceptResponse("in-flight", {colorQualities:[], frameRates:descriptors(true, "")})
        check(owner.settings.fps === 360, "the saved preference survives the descriptor reply")
        check(owner.selectableFpsValues("1920x1080").indexOf(360) >= 0,
            "the confirmed descriptor makes the tier selectable")
        capability(false)
        request(false, "Synthetic device cannot decode 360 FPS")
        check(owner.frameRateDescriptor(360).disabled === true, "the core device verdict is stored")
        check(owner.lockedFpsValues("1920x1080").indexOf(360) >= 0, "a device-locked rate is visible but locked")
        check(owner.lockedFpsValues("1920x1080").indexOf(240) < 0, "base rates stay selectable")
        check(owner.selectableFpsValues("1920x1080").indexOf(360) < 0, "a device-locked rate is not selectable")
        check(owner.resolveEntitledFps("1920x1080", 360) === 240,
            "a device-locked saved rate resolves to the nearest eligible rate")
        check(ShellStore.lockedFpsReason().indexOf("360") >= 0, "the capability reason reaches the surfaces")

        capability(true)
        request(true, "")
        check(owner.selectableFpsValues("1920x1080").indexOf(360) >= 0,
            "full HD with hardware decode and entitlement selects the top tier")
        check(owner.resolveEntitledFps("1920x1080", 360) === 360, "an eligible saved rate is preserved")
        check(ShellStore.lockedFpsReason() === "", "an eligible profile reports no capability reason")

        owner.colorRequestId = "dropped-verdict"
        acceptFailure("dropped-verdict", "Synthetic capability loss")
        check(owner.lockedFpsValues("1920x1080").indexOf(360) >= 0,
            "a lost capability verdict is not affirmative support")
        check(ShellStore.lockedFpsReason() === "Capability not confirmed",
            "the unconfirmed tier explains itself")
        ShellStore.subscription = null
        check(owner.lockedFpsValues("1920x1080").indexOf(360) >= 0,
            "an unloaded membership keeps the conditional tier locked")
        entitlement(1920, 1080, 360)
        request(true, "")

        owner.settings = Object.assign({}, owner.settings, {resolution:"2560x1440"})
        capability(true)
        request(false, "360 FPS is offered at full HD only. Choose a full HD resolution first.")
        check(owner.lockedFpsValues("2560x1440").indexOf(360) >= 0,
            "the documented full-HD ceiling locks the top tier at other resolutions")

        entitlement(2560, 1440, 120)
        check(owner.unentitledFpsValues("2560x1440").indexOf(360) >= 0
            && owner.unentitledFpsValues("2560x1440").indexOf(240) >= 0,
            "the membership still locks rates it does not entitle")
        check(owner.resolveEntitledFps("2560x1440", 360) === 120,
            "membership and device locks resolve together")

        entitlement(1920, 1080, 240)
        owner.settings = Object.assign({}, owner.settings, {resolution:"1920x1080"})
        capability(true)
        request(true, "")
        check(owner.entitledFpsForResolution("1920x1080").indexOf(360) < 0,
            "a 240 FPS entitlement never synthesizes the top tier")
        check(owner.lockedFpsValues("1920x1080").indexOf(360) >= 0,
            "the membership lock alone keeps the top tier out of reach")

        shell = find(parent, "desktopSettingsScreen")
        if (shell !== null) {
            control = find(shell, "desktopFrameRateControl")
            check(control !== null, "the desktop frame-rate selector exists")
            entitlement(1920, 1080, 360)
            owner.settings = Object.assign({}, owner.settings, {resolution:"1920x1080"})
            capability(true)
            request(true, "")
            check(optionEnabled(360) === true, "an entitled and capable profile enables the top tier chip")
            owner.settings = Object.assign({}, owner.settings, {resolution:"2560x1440"})
            capability(true)
            request(false, "360 FPS is offered at full HD only.")
            check(optionEnabled(360) === false, "the resolution verdict disables the top tier chip")
            entitlement(2560, 1440, 240)
            check(optionEnabled(240) === true, "an entitled rate the device can run stays selectable")
            check(optionEnabled(360) === false, "the membership lock keeps the top tier unreachable")
            owner.settings = Object.assign({}, owner.settings, {resolution:"1920x1080"})
        } else {
            shell = find(parent, "consoleSettingsScreen")
            check(shell !== null, "a production settings surface exists")
            check(shell.fpsChoices().indexOf(360) >= 0, "the console selector offers the canonical top rate")
            check(shell.fpsLockedValues().indexOf(360) >= 0, "the console shares the capability verdict")
            const model = shell.settingsModel()
            const row = model.find(item => item.key === "fps")
            check(row && row.values.indexOf(360) >= 0, "the console settings row carries the top rate")
            check(row && row.disabledValues.indexOf(360) >= 0, "the console settings row locks it")
            check(shell.fpsNote().indexOf("360") >= 0 || shell.fpsNote().indexOf("240") >= 0,
                "the console note reports the effective ceiling")
        }
        return true
    }
}

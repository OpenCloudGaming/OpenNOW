import QtQuick
import OpenNOW

QtObject {
    property var contentRoot: null
    property QtObject network: QtObject {
        property int state: MacAwdlController.Unsupported
        property bool busy: false
        property bool enabled: false
        property bool leased: false
        property string error: ""
        function refresh() {}
        function install() { throw new Error("Unsupported AWDL helper installation") }
        function uninstall() { throw new Error("Unsupported AWDL helper uninstallation") }
    }
    property QtObject client: QtObject {
        property var requests: []
        function request(method, params) {
            const id = "onboarding-test-" + (requests.length + 1)
            requests = requests.concat([{id: id, method: method, params: params}])
            return id
        }
    }

    function check(ok, message) {
        if (!ok)
            throw new Error("Onboarding: " + message)
    }

    function find(item, name) {
        if (item.objectName === name)
            return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found)
                return found
        }
        return null
    }

    function acknowledge() {
        const request = client.requests[client.requests.length - 1]
        ShellStore.settingsOwnerState.acceptResponse(request.id, {value: request.params.value})
    }

    function run(parent) {
        contentRoot = parent
        ShellStore.onboardingAwdlController = network
        ShellStore.authRestorePending = false
        ShellStore.authSession = {user: {displayName: "New player", userId: "onboarding-fixture"}}
        ShellStore.activeSession = null
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            onboardingCompleted: false, launchInConsoleMode: false,
            frameGeneration: "off", upscaling: "off", fps: 60, resolution: "1920x1080",
            codec: "auto", enableHdr: false, maxBitrateMbps: 75, reducedMotion: true
        })
        const owner = ShellStore.onboardingOwnerState
        ShellStore.settingsOwnerState.coreClient = client
        ShellStore.settingsOwnerState.ready = true
        owner.ready = true
        AppController.navigate("home")
        const screen = find(parent, "desktopOnboardingScreen")
        check(screen && screen.visible, "first-run wizard did not replace the shell")
        check(screen.stepCount === 4 && screen.stepIndex === 0 && screen.step === "mode", "wrong first step")
        check(String(screen.steps) === "mode,picture,boost,ready",
            "the network check appeared without a supported AWDL controller")
        for (let step = 1; step < 4; ++step) {
            screen.next()
            check(screen.stepIndex === step, "next did not reach step " + step)
        }
        screen.goToStep(screen.steps.indexOf("mode"))
        check(find(screen, "onboardingReportBug") && find(screen, "onboardingReportBug").visible,
            "the beta report-a-bug action is missing from Mode")
        ShellStore.setOnboardingSetting("launchInConsoleMode", true)
        check(ShellStore.onboardingSettings.launchInConsoleMode === true,
            "console choice was not staged")
        check(ShellStore.settings.launchInConsoleMode === false,
            "console choice switched the shell before saving")
        screen.goToStep(screen.steps.indexOf("picture"))
        check(!find(screen, "onboardingBitrate"), "Fine-tune controls were not folded by default")
        const fineTune = find(screen, "onboardingFineTune")
        check(fineTune, "Fine-tune disclosure missing")
        fineTune.expansionRequested()
        const bitrate = find(screen, "onboardingBitrate")
        check(bitrate, "bitrate control missing")
        bitrate.moved(85)
        check(ShellStore.onboardingSettings.maxBitrateMbps === 85, "bitrate did not update its real key")
        check(ShellStore.settingsOwnerState.capabilitiesActive === true,
            "the onboarding route activates the frame-rate choices lifecycle")
        const frameRate = find(screen, "onboardingFps")
        check(frameRate, "frame-rate control missing")
        const canonical = ShellStore.canonicalFpsValues()
        check(canonical.indexOf(360) >= 0, "the shared canonical rates expose the documented top tier")
        for (const value of canonical)
            check(frameRate.options.some(option => frameRate.optionValue(option) === value),
                "the onboarding selector offers the shared rate " + value)
        const fasterIndex = frameRate.options.findIndex(option => frameRate.optionValue(option) === 120)
        frameRate.selected(fasterIndex, frameRate.options[fasterIndex])
        check(ShellStore.onboardingSettings.fps === 120, "frame rate did not stage its real key")
        screen.goToStep(screen.steps.indexOf("boost"))
        const generation = find(screen, "onboardingFrameGeneration")
        check(generation && generation.selectedIndex === 0, "frame generation did not default off")
        generation.selected(1, generation.options[1])
        check(ShellStore.onboardingSettings.frameGeneration === "2x", "2x selection was not staged")
        check(ShellStore.settings.frameGeneration === "off", "frame generation saved prematurely")
        const upscaling = find(screen, "onboardingUpscaling")
        const upscaleMode = Qt.platform.os === "osx" ? "metalfx" : "fsr1"
        check(upscaling && upscaling.selectedIndex === 0, "upscaling did not default off")
        check(upscaling.options[1].value === upscaleMode, "onboarding offers the wrong platform upscaler")
        upscaling.selected(1, upscaling.options[1])
        check(ShellStore.onboardingSettings.upscaling === upscaleMode, "upscaling selection was not staged")
        check(ShellStore.settings.upscaling === "off", "upscaling saved prematurely")
        const clarity = find(screen, "onboarding-upscalingSharpness")
        check(clarity && clarity.enabled, "upscaling clarity control is missing or disabled")
        clarity.moved(8)
        check(ShellStore.onboardingSettings.upscalingSharpness === 8, "clarity selection was not staged")
        find(screen, "onboardingSkipBoost").clicked()
        check(screen.step === "ready", "Skip for now did not continue to Ready")
        check(ShellStore.onboardingSettings.frameGeneration === "off"
            && ShellStore.onboardingSettings.upscaling === "off"
            && ShellStore.onboardingSettings.upscalingSharpness === ShellStore.settings.upscalingSharpness,
            "Skip for now kept the staged boost choices")
        check(ShellStore.onboardingSettings.fps === 120 && ShellStore.onboardingSettings.launchInConsoleMode === true,
            "Skip for now discarded choices from other steps")
        screen.goToStep(screen.steps.indexOf("boost"))
        const generationAgain = find(screen, "onboardingFrameGeneration")
        generationAgain.selected(1, generationAgain.options[1])
        find(screen, "onboardingUpscaling").selected(1, find(screen, "onboardingUpscaling").options[1])
        find(screen, "onboarding-upscalingSharpness").moved(8)
        screen.goToStep(screen.steps.indexOf("ready"))
        check(find(screen, "onboardingSupport") && find(screen, "onboardingSupport").visible,
            "the optional support card is missing from Ready")
        screen.next()
        check(ShellStore.onboardingSaving, "finish did not begin persistence")
        check(client.requests[0].params.key === "launchInConsoleMode", "first choice was not saved first")
        ShellStore.settingsOwnerState.acceptFailure(owner.requestId, "Simulated write failure")
        check(!ShellStore.onboardingSaving && ShellStore.onboardingRequired,
            "failed save incorrectly completed onboarding")
        check(ShellStore.onboardingError.length > 0 && screen.visible, "retry error was not shown")
        screen.next()
        for (let i = 0; i < 16 && owner.saving; ++i)
            acknowledge()
        check(!owner.saving && !ShellStore.onboardingRequired, "retry did not complete")
        check(ShellStore.settings.launchInConsoleMode === true, "console choice did not persist")
        check(ShellStore.settings.frameGeneration === "2x", "frame generation did not persist")
        check(ShellStore.settings.upscaling === upscaleMode, "upscaling did not persist")
        check(ShellStore.settings.upscalingSharpness === 8, "clarity did not persist")
        check(client.requests[client.requests.length - 1].params.key === "onboardingCompleted",
            "completion marker was not saved last")
        check(!find(parent, "desktopOnboardingScreen"), "wizard remained after save acknowledgement")

        ShellStore.settings = Object.assign({}, ShellStore.settings, {onboardingCompleted: false})
        const skipped = find(parent, "desktopOnboardingScreen")
        check(skipped, "new first-run flag did not present wizard")
        skipped.skip()
        check(owner.saving && client.requests[client.requests.length - 1].params.key === "onboardingCompleted",
            "skip did not persist completion with existing defaults")
        acknowledge()
        check(!ShellStore.onboardingRequired, "skip did not finish")

        const args = Qt.application.arguments
        const index = args.indexOf("--onboarding-step")
        if (index >= 0) {
            const light = args.indexOf("--smoke-light-theme") >= 0
            const scaleIndex = args.indexOf("--onboarding-ui-scale")
            ShellStore.settings = Object.assign({}, ShellStore.settings, {
                onboardingCompleted: false, launchInConsoleMode: false, switchToConsoleOnPad: false,
                resolution: "1920x1080", fps: 60, codec: "auto", enableHdr: false, maxBitrateMbps: 75,
                frameGeneration: "off", upscaling: "off", upscalingSharpness: 10, upscalingDenoise: 0, themePack: light ? "cobalt" : "nocturne",
                appTheme: light ? "light" : "dark",
                desktopUiScale: scaleIndex >= 0 ? Number(args[scaleIndex + 1]) : 1
            })
            const shot = find(parent, "desktopOnboardingScreen")
            check(shot, "screenshot wizard missing")
            shot.goToStep(Number(args[index + 1] || 0))
        }
        if (args.indexOf("--onboarding-login") >= 0) {
            ShellStore.settings = Object.assign({}, ShellStore.settings, {launchInConsoleMode: false})
            ShellStore.consoleSurfaceRequested(false)
            ShellStore.authSession = null
            ShellStore.authRestorePending = false
            AppController.navigate("sign-in")
        }
        ShellStore.lastError = ""
        return true
    }

    function verify() {
        if (Qt.application.arguments.indexOf("--onboarding-login") >= 0) {
            const login = find(contentRoot, "desktopSignInScreen")
            check(login && login.visible, "desktop sign-in screen is not visible")
        }
        const screen = find(contentRoot, "desktopOnboardingScreen")
        if (!screen)
            return true
        const next = find(screen, "onboardingNext")
        const skip = find(screen, "onboardingSkip")
        check(next && next.width >= 120, "primary action collapsed: " + (next ? next.width : "missing"))
        check(skip && skip.width >= 80, "skip action collapsed")
        check(next.contentItem.truncated === false, "primary action label was clipped")
        check(skip.contentItem.truncated === false, "skip label was clipped")
        check(next.y + next.height <= next.parent.height, "primary action is outside its footer")
        if (screen.step === "mode") {
            const page = find(screen, "onboardingStepLoader").item
            const panels = page.children.filter(child => child.visible && child.height > 0)
            check(panels.length === 2, "expected the mode cards and the gamepad panel")
            check(panels.every(panel => panel.width >= 200), "mode panel collapsed")
            const a = panels[0]
            const b = panels[1]
            check(a.x + a.width <= b.x + 1 || b.x + b.width <= a.x + 1
                || a.y + a.height <= b.y + 1 || b.y + b.height <= a.y + 1,
                "mode panels overlap")
        }
        return true
    }
}

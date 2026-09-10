import QtQuick
import OpenNOW

QtObject {
    id: root
    property OnboardingAcceptance acceptance: OnboardingAcceptance {}
    property var contentRoot: null
    property var page: null

    function run(parent) {
        contentRoot = parent
        ShellStore.authRestorePending = false
        ShellStore.authSession = {user: {displayName: "Setup fixture", userId: "onboarding-replay-fixture"}}
        ShellStore.activeSession = null
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            onboardingCompleted: true, desktopUiScale: 1.25, reducedMotion: true
        })
        const owner = ShellStore.onboardingOwnerState
        owner.coreClient = acceptance.client
        owner.ready = true
        owner.replayAllowed = true
        AppController.navigate("settings")
        const screen = acceptance.find(parent, "desktopSettingsScreen")
        acceptance.check(screen, "settings did not open")
        screen.selectedSection = 11
        page = acceptance.find(screen, "settingsPageLoader").item
        const button = acceptance.find(page, "replayOnboardingButton")
        acceptance.check(button && button.enabled, "replay action is missing or disabled")
        let flick = button.parent
        while (flick && !(flick instanceof Flickable))
            flick = flick.parent
        acceptance.check(flick, "About is not scrollable")
        flick.contentY = Math.max(0, flick.contentHeight - flick.height)
        button.clicked()
        return true
    }

    function verify() {
        const confirmation = page.data.find(item => item.objectName === "replayOnboardingConfirmation")
        acceptance.check(confirmation && confirmation.opened, "replay confirmation did not open")
        const owner = ShellStore.onboardingOwnerState
        confirmation.reject()
        acceptance.check(acceptance.client.requests.length === 0, "cancel reset onboarding")
        confirmation.open()
        confirmation.accept()
        acceptance.check(owner.replaying && acceptance.client.requests.length === 1,
            "confirmation did not request a single reset")
        acceptance.check(!owner.needed, "reset changed the visible shell before persistence")
        const request = acceptance.client.requests[0]
        acceptance.check(request.params.key === "onboardingCompleted" && request.params.value === false,
            "replay wrote the wrong setting")
        owner.acceptFailure(request.id, "Fixture denied settings persistence")
        acceptance.check(!owner.replaying && ShellStore.onboardingReplayError.length > 0,
            "failed reset did not remain available for retry")
        acceptance.check(acceptance.find(page, "replayOnboardingButton").enabled,
            "failed reset disabled retry")
        owner.replayError = ""
        if (Qt.application.arguments.indexOf("--onboarding-replay-dialog") >= 0)
            confirmation.open()
        return true
    }
}

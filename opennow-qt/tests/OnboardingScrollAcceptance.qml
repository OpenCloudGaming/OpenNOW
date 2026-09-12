import QtQuick
import OpenNOW

QtObject {
    property OnboardingAcceptance acceptance: OnboardingAcceptance {}
    property var contentRoot: null

    function run(parent) {
        contentRoot = parent
        const passed = acceptance.run(parent)
        if (Qt.application.arguments.indexOf("--onboarding-login") >= 0)
            ShellStore.settings = Object.assign({}, ShellStore.settings, {desktopUiScale: 1.25})
        return passed
    }

    function verify() {
        acceptance.verify()
        const login = Qt.application.arguments.indexOf("--onboarding-login") >= 0
        const screen = acceptance.find(contentRoot, login ? "desktopSignInScreen" : "desktopOnboardingScreen")
        const scroll = acceptance.find(screen, login ? "signInScroll" : "onboardingScroll")
        const page = login ? scroll.contentItem : acceptance.find(screen, "onboardingStepLoader").item
        if (login) {
            for (const name of ["signInProviderName", "signInProviderRegion"]) {
                const label = acceptance.find(screen, name)
                acceptance.check(label && label.text.length > 0 && label.height > 0 && label.lineCount > 0,
                    "provider label collapsed in the sign-in card")
            }
        }
        acceptance.check(scroll.contentHeight > scroll.height, "compact page did not expose its scrollable content")
        const candidates = []
        function collect(item) {
            if (!item.visible || !item.enabled)
                return
            if (item.activeFocusOnTab && item.height > 0 && item.height < scroll.height)
                candidates.push(item)
            for (const child of item.children || [])
                collect(child)
        }
        collect(page)
        acceptance.check(candidates.length > 0, "compact page has no reachable controls")
        let furthest = 0
        for (const control of candidates) {
            control.forceActiveFocus(Qt.TabFocusReason)
            screen.revealFocusedControl()
            const point = control.mapToItem(scroll, 0, 0)
            acceptance.check(point.y >= -1 && point.y + control.height <= scroll.height + 1,
                "focused control is outside the compact viewport")
            acceptance.check(point.x >= -1 && point.x + control.width <= scroll.width + 1,
                "focused control is horizontally clipped")
            furthest = Math.max(furthest, scroll.contentY)
        }
        acceptance.check(furthest > 0, "focusing controls did not scroll the compact page")
        return true
    }
}

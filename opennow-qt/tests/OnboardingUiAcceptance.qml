import QtQuick
import OpenNOW

QtObject {
    property OnboardingAcceptance acceptance: OnboardingAcceptance {}
    property var contentRoot: null

    function run(parent) {
        contentRoot = parent
        acceptance.run(parent)
        const screen = acceptance.find(parent, "desktopOnboardingScreen")
        acceptance.check(screen, "UI fixture needs an onboarding step")
        const step = screen.stepIndex
        screen.goToStep(2)
        const picker = acceptance.find(screen, "onboardingResolution")
        acceptance.check(picker && picker.groups.length > 0 && picker.available.length > 1,
            "resolution picker lost the grouped settings choices")
        const persisted = ShellStore.settings.resolution
        const requests = acceptance.client.requests.length
        const original = picker.value
        const choice = picker.available.find(item => item.value !== original)
        picker.selected(choice.value)
        acceptance.check(ShellStore.onboardingSettings.resolution === choice.value,
            "resolution selection did not update the draft")
        acceptance.check(picker.value === choice.value, "resolution picker did not follow the draft")
        picker.step(1)
        acceptance.check(picker.available.some(item => item.value === picker.value),
            "resolution stepper selected an unavailable choice")
        acceptance.check(ShellStore.settings.resolution === persisted
            && acceptance.client.requests.length === requests,
            "resolution selection persisted before finishing setup")
        picker.selected(original)
        screen.goToStep(step)
        if (Qt.application.arguments.indexOf("--onboarding-resolution-expanded") >= 0) {
            acceptance.check(step === 2, "expanded resolution capture needs Picture")
            acceptance.find(screen, "onboardingResolution").expanded = true
        }
        return true
    }

    function verify() {
        acceptance.verify()
        const screen = acceptance.find(contentRoot, "desktopOnboardingScreen")
        const scroll = acceptance.find(screen, "onboardingScroll")
        const body = acceptance.find(screen, "onboardingPageBody")
        const point = scroll.mapToItem(screen, 0, 0)
        const railWidth = screen.compact ? 0 : DesktopTokens.px(236)
        acceptance.check(Math.abs((point.x - railWidth) - (screen.width - point.x - scroll.width)) <= 1,
            "onboarding page has unequal horizontal gutters")
        if (body.height + 2 * scroll.pageMargin <= scroll.height) {
            acceptance.check(Math.abs(body.y - (scroll.height - body.height) / 2) <= 1,
                "short onboarding page is not vertically centered")
            acceptance.check(Math.abs(scroll.contentHeight - scroll.height) <= 1,
                "short onboarding page scrolls unnecessarily")
        } else {
            acceptance.check(Math.abs(body.y - scroll.pageMargin) <= 1,
                "overflowing onboarding page lost its top margin")
            acceptance.check(scroll.contentHeight >= body.y + body.height,
                "overflowing onboarding page is not fully scrollable")
        }
        const picker = acceptance.find(screen, "onboardingResolution")
        if (picker && picker.expanded) {
            const controls = []
            function collect(item) {
                if (!item.visible || !item.enabled)
                    return
                if (item.activeFocusOnTab && item.height > 0 && item.height < scroll.height)
                    controls.push(item)
                for (const child of item.children || [])
                    collect(child)
            }
            collect(picker)
            acceptance.check(controls.length > picker.available.length,
                "expanded resolution choices are not keyboard reachable")
            for (const control of controls) {
                control.forceActiveFocus(Qt.TabFocusReason)
                screen.revealFocusedControl()
                let ancestor = control.parent
                while (ancestor) {
                    if (ancestor instanceof Flickable) {
                        const position = control.mapToItem(ancestor, 0, 0)
                        acceptance.check(position.y >= -1
                            && position.y + control.height <= ancestor.height + 1,
                            "focused resolution control is vertically clipped")
                        acceptance.check(position.x >= -1
                            && position.x + control.width <= ancestor.width + 1,
                            "focused resolution control is horizontally clipped")
                    }
                    if (ancestor === scroll)
                        break
                    ancestor = ancestor.parent
                }
            }
            controls[0].forceActiveFocus(Qt.TabFocusReason)
            picker.children.find(item => item instanceof Flickable).contentY = 0
            scroll.contentY = 0
        }
        return true
    }
}

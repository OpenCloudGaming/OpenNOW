import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    property OnboardingAcceptance acceptance: OnboardingAcceptance {}
    property var contentRoot: null
    property var card: null
    property QtObject controller: QtObject {
        property int state: MacAwdlController.Enabled
        property bool busy: false
        property string error: ""
        property int disableRequests: 0
        property int enableRequests: 0
        property int refreshRequests: 0
        function refresh() { ++refreshRequests }
        function disable() { ++disableRequests; busy = true }
        function enable() { ++enableRequests; busy = true }
    }

    function objectIn(item, name) {
        if (item.objectName === name)
            return item
        for (const child of item.data || item.children || []) {
            const found = objectIn(child, name)
            if (found)
                return found
        }
        return null
    }

    function run(parent) {
        contentRoot = parent
        acceptance.run(parent)
        const screen = acceptance.find(parent, "desktopOnboardingScreen")
        acceptance.check(screen, "AWDL fixture needs an onboarding screen")
        screen.goToStep(3)
        card = acceptance.find(screen, "onboardingMacNetwork")
        acceptance.check(card, "AWDL guidance card is missing")
        card.controller = controller
        if (Qt.application.arguments.indexOf("--onboarding-awdl-fullscreen") >= 0)
            parent.Window.window.showFullScreen()
        return true
    }

    function verify() {
        acceptance.verify()
        if (Qt.application.arguments.indexOf("--onboarding-awdl-fullscreen") >= 0)
            acceptance.check(contentRoot.Window.window.visibility === Window.FullScreen,
                "AWDL fullscreen fixture did not enter fullscreen")
        const change = acceptance.find(card, "onboardingAwdlChange")
        const refresh = acceptance.find(card, "onboardingAwdlRefresh")
        const confirmation = objectIn(card, "onboardingAwdlConfirmation")
        acceptance.check(confirmation, "AWDL confirmation dialog is missing")
        acceptance.check(card.visible && change.enabled, "active AWDL has no available test action")
        acceptance.check(controller.disableRequests === 0, "AWDL changed before explicit consent")
        change.clicked()
        acceptance.check(confirmation.visible, "AWDL change did not ask for confirmation")
        const cancel = acceptance.find(contentRoot, "onboardingAwdlCancel")
        const confirm = acceptance.find(contentRoot, "onboardingAwdlConfirm")
        acceptance.check(cancel && confirm, "AWDL confirmation actions are missing")
        cancel.clicked()
        acceptance.check(!confirmation.visible, "cancel did not dismiss AWDL confirmation")
        acceptance.check(controller.disableRequests === 0, "canceling confirmation changed AWDL")
        change.clicked()
        confirm.clicked()
        acceptance.check(controller.disableRequests === 1, "confirmed disable did not reach its controller")
        acceptance.check(!change.enabled && !refresh.enabled, "AWDL allowed duplicate requests while authorizing")
        controller.state = MacAwdlController.Disabled
        controller.busy = false
        acceptance.check(change.enabled, "down AWDL has no restore action")
        change.clicked()
        confirm.clicked()
        acceptance.check(controller.enableRequests === 1, "confirmed restore did not reach its controller")
        controller.busy = false
        controller.state = MacAwdlController.Enabled
        controller.error = "Authorization canceled by test fixture"
        const error = acceptance.find(card, "onboardingAwdlError")
        acceptance.check(error.visible && error.text === controller.error, "authorization error was hidden")
        controller.state = MacAwdlController.Unknown
        acceptance.check(!change.enabled && refresh.enabled, "unknown AWDL state allowed a change")
        const reads = controller.refreshRequests
        refresh.clicked()
        acceptance.check(controller.refreshRequests === reads + 1, "refresh did not recheck AWDL")
        controller.state = MacAwdlController.Unavailable
        acceptance.check(!change.enabled, "missing AWDL interface allowed a change")
        controller.state = MacAwdlController.Unsupported
        acceptance.check(!card.visible, "AWDL guidance is visible on an unsupported platform")
        controller.error = ""
        controller.state = MacAwdlController.Enabled
        const screen = acceptance.find(contentRoot, "desktopOnboardingScreen")
        const scroll = acceptance.find(screen, "onboardingScroll")
        change.forceActiveFocus(Qt.TabFocusReason)
        screen.revealFocusedControl()
        const point = change.mapToItem(scroll, 0, 0)
        acceptance.check(point.y >= -1 && point.y + change.height <= scroll.height + 1,
            "AWDL action is outside the focused viewport")
        acceptance.check(point.x >= -1 && point.x + change.width <= scroll.width + 1,
            "AWDL action is horizontally clipped")
        if (Qt.application.arguments.indexOf("--onboarding-awdl-confirmation") >= 0)
            change.clicked()
        return true
    }
}

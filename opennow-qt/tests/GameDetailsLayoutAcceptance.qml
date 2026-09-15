import QtQuick
import OpenNOW

OwnershipAcceptance {
    id: root
    property var host: null
    property var modal: null
    property var scroll: null
    property var primary: null
    property var controls: []
    property int controlIndex: 0
    property int phase: 0
    property int key: 0
    property bool settling: false
    onKeyChanged: if (key !== 0) settling = true
    property int modifiers: 0
    readonly property bool owned: Qt.application.arguments.indexOf("--details-owned") >= 0

    function inside(item, bounds) {
        const top = item.mapToItem(bounds, 0, 0)
        const edge = item.mapToItem(bounds, item.width, item.height)
        check(top.x >= -1 && top.y >= -1 && edge.x <= bounds.width + 1 && edge.y <= bounds.height + 1,
            item.objectName + " exceeds its bounds: " + [top.x, top.y, edge.x, edge.y, bounds.width, bounds.height])
    }
    function buttons(item) {
        let found = []
        for (const child of item.children || []) {
            if (!child.visible || !child.enabled) continue
            if (child.activeFocusOnTab && typeof child.clicked === "function") found.push(child)
            else found = found.concat(buttons(child))
        }
        return found
    }
    function contained(item) {
        for (const child of item.children || []) {
            if (!child.visible) continue
            inside(child, item)
            contained(child)
        }
    }
    function unobscured(control) {
        const close = find(modal, "gameDetailsClose")
        const top = control.mapToItem(modal, 0, 0)
        const corner = close.mapToItem(modal, 0, 0)
        check(top.x + control.width <= corner.x || top.x >= corner.x + close.width
            || top.y + control.height <= corner.y || top.y >= corner.y + close.height,
            "close button covers " + control.objectName)
    }
    function run(windowHost) {
        host = windowHost
        const scaleIndex = Qt.application.arguments.indexOf("--details-scale")
        const scale = scaleIndex >= 0 ? Number(Qt.application.arguments[scaleIndex + 1]) : 1
        ShellStore.settings = Object.assign({}, ShellStore.settings, {onboardingCompleted:true, desktopUiScale:scale,
            resolution:"1920x1080", fps:60, codec:"h264"})
        DesktopTokens.uiScale = scale
        ShellStore.authGeneration = 42
        ShellStore.authSession = {user:{userId:"fixture-user",displayName:"Layout fixture"},provider:{idpId:"fixture-provider",code:"NVIDIA"}}
        ShellStore.nativeRuntimeReady = true
        const owner = ShellStore.catalogOwnerState
        owner.resetCloudActions()
        const selected = game(owned ? 0 : 1)
        selected.title = owned ? "Single-store game" : "A multi-store game with a longer title"
        selected.publisherName = owned ? "Example publisher" : "Example publisher with a longer name"
        selected.heroImageUrl = "qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"
        if (owned) {
            selected.variants = selected.variants.slice(0, 1)
            selected.variants[0].libraryStatus = "PLATFORM_SYNC"
        } else {
            selected.variants.push({id:"789",store:"XBOX",libraryStatus:"NOT_OWNED",librarySelected:false,gfnStatus:"AVAILABLE",playStatus:"UNKNOWN"})
        }
        selected.availableStores = selected.variants.map(variant => variant.store)
        owner.catalogGames = [selected]
        owner.catalogState = "ready"
        owner.catalogComplete = true
        ShellStore.selectedGame = selected
        AppController.navigate("game-detail")
        primary = bindDetail(host)
        detail(owned ? "ready" : "ownership_required")
        if (!owned) owner.selectedLaunchDecision = {status:"ownership_required",
            message:"Confirm that you already own this store version before adding it to your GeForce NOW library. This does not buy the game or grant a license."}
        modal = find(host, "desktopGameModal")
        scroll = find(modal, "gameDetailsScroll")
        return true
    }
    function advance() {
        if (settling) {
            settling = false
            return 0
        }
        if (phase === 0) {
            inside(primary, scroll)
            inside(find(modal, "gameDetailsClose"), modal)
            const body = find(modal, "gameDetailsBody")
            const sections = [find(modal, "gameDetailsReadiness"), find(modal, "gameDetailsPrimaryActions"),
                find(modal, "cloudLibraryActions"), find(modal, "gameDetailsSummary")]
            for (let i = 0; i < sections.length; ++i) {
                inside(sections[i], body)
                if (i > 0) check(sections[i].y - sections[i - 1].y - sections[i - 1].height >= DesktopTokens.px(20) - 1,
                    "adjacent sections lost their scaled vertical spacing")
            }
            const summary = sections[3]
            const bottom = summary.mapToItem(scroll.contentItem, 0, summary.height).y
            check(Math.abs(scroll.contentHeight - bottom - DesktopTokens.px(24)) <= 1, "bottom padding is missing from the scroll extent")
            for (const card of summary.children) {
                if (!card.visible || card.width <= 0 || card.height <= 0) continue
                inside(card, summary)
                if (card.objectName === "gameDetailsSummaryCard") contained(card)
            }
            controls = buttons(body)
            check(controls.length >= 7 && controls[controls.length - 1].objectName === "gameDetailsTune", "missing detail controls")
            controls[0].forceActiveFocus()
            phase = 1
            return 0
        }
        if (phase === 1) {
            const control = controls[controlIndex]
            check(control.activeFocus, "Tab did not reach " + control.objectName + " " + control.text)
            inside(control, scroll)
            unobscured(control)
            if (++controlIndex < controls.length) {
                key = Qt.Key_Tab
                return 0
            }
            check(scroll.contentHeight - DesktopTokens.px(24) <= scroll.height || scroll.contentY > 0,
                "keyboard focus did not reveal the footer")
            scroll.contentY = Math.max(0, scroll.contentHeight - scroll.height)
            phase = 2
            return 0
        }
        if (phase === 2) {
            inside(find(modal, "gameDetailsSummary"), scroll)
            if (!owned) {
                find(modal, "desktopStoreVariant0").forceActiveFocus()
                key = Qt.Key_Return
                phase = 3
                return 0
            }
            phase = 4
            primary.forceActiveFocus()
            key = Qt.Key_Return
            return 0
        }
        if (phase === 3) {
            check(ShellStore.selectedGame.selectedVariantIndex === 0, "keyboard selection changed the wrong store")
            ShellStore.selectGameVariant(1)
            detail("ownership_required")
            primary.forceActiveFocus()
            key = Qt.Key_Return
            phase = 4
            return 0
        }
        if (phase === 4) {
            if (owned) {
                check(ShellStore.launchInspectRequestId !== "", "owned action bypassed launch inspection")
                client.requestFailed(ShellStore.launchInspectRequestId, "network_error", "Fixture stops before session allocation")
            } else {
                check(ShellStore.ownershipConfirmation && ShellStore.ownershipConfirmation.variantId === "456",
                    "unowned action did not confirm the selected store version")
                ShellStore.ownershipConfirmation = null
            }
            check(count("session.create") === 0, "layout interactions allocated a session")
            primary.forceActiveFocus()
            key = Qt.Key_Escape
            phase = 5
            return 0
        }
        if (phase === 5) {
            check(!modal.opened, "Escape did not dismiss details")
            AppController.navigate("game-detail")
            detail(owned ? "ready" : "ownership_required")
            phase = 6
            return 0
        }
        if (phase === 6) {
            inside(primary, scroll)
            check(primary.activeFocus, "reopening did not restore primary action focus")
            if (Qt.application.arguments.indexOf("--details-bottom") >= 0)
                scroll.contentY = Math.max(0, scroll.contentHeight - scroll.height)
            phase = 7
            return 0
        }
        return 1
    }
}

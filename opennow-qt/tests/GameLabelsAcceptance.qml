import QtQuick
import OpenNOW

QtObject {
    function check(value, message) { if (!value) throw new Error("Game labels: " + message) }
    function collect(item, name, found) {
        if (item.objectName === name) found.push(item)
        for (const child of item.children || []) collect(child, name, found)
        return found
    }
    function labelsFor(host, id) {
        return collect(host, "desktopGameLabels", []).find(item => item.game && item.game.id === id && item.visible)
    }
    function child(item, name) { return collect(item, name, [])[0] }
    function effectiveVisible(item) {
        for (let node = item; node; node = node.parent)
            if (node.visible === false || node.opacity === 0) return false
        return true
    }
    function visibleTexts(item, found) {
        found = found || []
        if (typeof item.text === "string" && effectiveVisible(item)) found.push(item)
        for (const nested of item.children || []) visibleTexts(nested, found)
        return found
    }
    function polish(item) {
        for (const nested of item.children || []) polish(nested)
        if (typeof item.forceLayout === "function") item.forceLayout()
    }
    function variant(id, store, owned, features) {
        return {id:id, store:store, libraryStatus:owned ? "PLATFORM_SYNC" : "NOT_OWNED", inLibrary:owned,
            gfnStatus:"AVAILABLE", playStatus:owned ? "PLAYABLE" : "UNKNOWN", features:features === undefined ? null : features}
    }
    function game(id, title, variants, extra) {
        return Object.assign({id:id, uuid:id, title:title, playabilityState:"PLAYABLE", selectedVariantIndex:0,
            isInLibrary:true, availableStores:variants.map(item => item.store),
            imageUrl:"qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg", variants:variants}, extra || {})
    }
    readonly property var crowded: game("labels-crowded", "Four launchers", [
        variant("101", "Steam", false, ["rtx"]),
        variant("102", "Epic Games Store", false, ["hdr", "dlss"]),
        variant("103", "GOG", true, ["reflex"]),
        variant("104", "Xbox", false),
        variant("105", "Battle.net", false),
        variant("106", "Steam", false)])
    readonly property var locked: game("labels-locked", "Paid membership", [variant("201", "Steam", true)],
        {requiresPaidMembership:true, membershipTierLabel:"Premium"})
    readonly property var plain: game("labels-plain", "No labels", [variant("301", "Unknown", false)])
    readonly property var upscaled: game("labels-dlss", "DLSS listed", [variant("401", "Steam", true, ["rtx"])],
        {dlss:{superResolution:"override", frameGeneration:"native", multiFrameGeneration:{maxMultiplier:4},
            rayReconstruction:null, dlaa:null, source:"nvidia-rtx-games-list"}})
    readonly property var stacked: game("labels-stacked", "Four features", [variant("501", "Steam", true, ["reflex", "rtx", "hdr"])],
        {dlss:{superResolution:"override", frameGeneration:"native", multiFrameGeneration:null, rayReconstruction:null,
            dlaa:null, source:"nvidia-rtx-games-list"}})

    function lockedFor(tier) {
        ShellStore.subscription = tier === null ? null : {membershipTier:tier}
        return ShellStore.gameLockedByMembership(locked)
    }

    function run(host) {
        const launchers = ConsoleStores.launchers(crowded)
        check(launchers.length === 5, "duplicate stores were not merged")
        check(launchers[0].store === "GOG" && launchers[0].owned && !launchers.slice(1).some(item => item.owned),
            "the owned launcher does not lead")
        check(JSON.stringify(launchers.slice(1).map(item => item.store)) === JSON.stringify(["Steam", "Epic Games Store", "Xbox", "Battle.net"]),
            "unowned launchers lost catalog order")
        check(JSON.stringify(DesktopTokens.gameFeatures(crowded)) === JSON.stringify(["rtx", "hdr", "reflex"]),
            "features are not the data-backed set in fixed order")
        check(DesktopTokens.gameFeatures(plain).length === 0 && ConsoleStores.launchers(plain).length === 0,
            "a game without catalog data shows labels")
        check(JSON.stringify(DesktopTokens.gameFeatures(game("labels-unknown", "Unknown features",
                [variant("501", "Steam", true, null), variant("502", "Xbox", false, [])]))) === "[]",
            "variants with unknown (null) or empty features show feature labels")
        check(JSON.stringify(DesktopTokens.gameFeatures(upscaled)) === JSON.stringify(["rtx", "dlss"]),
            "a game with core DLSS data does not list DLSS after its catalog features")
        check(JSON.stringify(DesktopTokens.gameFeatures(stacked)) === JSON.stringify(["rtx", "hdr", "reflex", "dlss"]),
            "a game with three catalog features and DLSS does not list all four in fixed order")
        check(DesktopTokens.gameFeatures(Object.assign({}, upscaled, {dlss:null})).indexOf("dlss") < 0,
            "a game the DLSS list does not match shows DLSS")
        check(DesktopTokens.featureTip("dlss") === "DLSS — per NVIDIA's DLSS games list"
            && DesktopTokens.featureTip("rtx") === DesktopTokens.featureLabel("rtx"),
            "the DLSS tooltip does not name its source")

        ShellStore.authSession = {user:{userId:"labels-user", displayName:"Labels fixture"}, provider:{idpId:"fixture", code:"NVIDIA"}}
        check(lockedFor("FREE"), "a Free account is not locked out of a paid game")
        check(!lockedFor("PERFORMANCE") && !lockedFor("ULTIMATE"), "a paid account is locked out")
        check(!lockedFor(null), "an unknown tier is locked out")
        check(!ShellStore.gameLockedByMembership(Object.assign({}, locked, {requiresPaidMembership:false})) ,
            "a game the catalog does not gate is locked")
        lockedFor("FREE")
        check(!ShellStore.gameLockedByMembership(crowded), "an ungated game is locked for a Free account")

        const owner = ShellStore.catalogOwnerState
        ShellStore.settings = Object.assign({}, ShellStore.settings, {onboardingCompleted:true})
        owner.catalogGames = [crowded, locked, plain, upscaled, stacked]
        owner.catalogState = "ready"
        owner.catalogComplete = true
        return true
    }

    function verifyRendered(host) {
        const crowdedLabels = labelsFor(host, "labels-crowded")
        check(crowdedLabels, "crowded tile labels were not rendered")
        check(crowdedLabels.shownLaunchers === 3, "a tile shows more than three launchers")
        const overflow = child(crowdedLabels, "desktopGameLauncherOverflow")
        check(overflow && overflow.visible && overflow.text === "+2", "launcher overflow is not +2")
        const first = child(crowdedLabels, "desktopGameLauncher0")
        check(first && first.launcher.store === "GOG" && first.opacity === 1, "the owned launcher is not first and bright")
        check(child(first, "desktopGameOwnedDot").visible, "the owned launcher has no green dot")
        const second = child(crowdedLabels, "desktopGameLauncher1")
        check(second && second.opacity < 1 && !child(second, "desktopGameOwnedDot").visible, "an unowned launcher looks owned")
        check(!child(crowdedLabels, "desktopGameLock").visible, "an ungated game shows a lock")
        check(!child(crowdedLabels, "desktopGameFeature-dlss"), "a variant's catalog features drew DLSS without game.dlss")
        const features = child(crowdedLabels, "desktopGameFeatures")
        const launcherPill = child(crowdedLabels, "desktopGameLaunchers")
        function separated() {
            return launcherPill.x + launcherPill.width < features.x && features.x + features.width <= crowdedLabels.width + 0.5
        }
        check(separated(), "the feature pill overlaps the launchers or overflows the tile")
        crowdedLabels.width = DesktopTokens.posterWidth - 16
        polish(crowdedLabels)
        check(crowdedLabels.shownLaunchers >= 1 && separated(), "the pills overlap on the narrowest Home tile")

        const stackedLabels = labelsFor(host, "labels-stacked")
        check(stackedLabels, "the four-feature tile labels were not rendered")
        const stackedFeatures = child(stackedLabels, "desktopGameFeatures")
        const stackedLaunchers = child(stackedLabels, "desktopGameLaunchers")
        const stackedChip = child(stackedLabels, "desktopGameFeatureOverflow")
        stackedLabels.width = Math.round(DesktopTokens.posterWidth * 1.5)
        polish(stackedLabels)
        check(stackedLabels.shownFeatures === 4 && !stackedChip.visible,
            "the four-feature tile does not draw all four glyphs when the tile has room")
        stackedLabels.width = DesktopTokens.posterWidth - 16
        polish(stackedLabels)
        check(stackedLabels.shownFeatures < 4 && stackedChip.visible
            && stackedChip.text === "+" + (4 - stackedLabels.shownFeatures),
            "a four-feature tile does not fall back to the +N chip on the narrowest Home tile")
        check(stackedLaunchers.x + stackedLaunchers.width < stackedFeatures.x
            && stackedFeatures.x + stackedFeatures.width <= stackedLabels.width + 0.5,
            "the +N chip overlaps the launchers or overflows the narrowest Home tile")

        const lockedLabels = labelsFor(host, "labels-locked")
        check(lockedLabels && child(lockedLabels, "desktopGameLock").visible, "a paid game shows no lock for a Free account")
        check(lockedLabels.summary.indexOf("Needs a paid membership") === 0, "the lock is missing from the accessible description")
        ShellStore.subscription = {membershipTier:"ULTIMATE"}
        check(!child(lockedLabels, "desktopGameLock").visible, "the lock stayed after the tier changed")

        const tipAnchor = collect(host, "desktopGameLabelTipAnchor", []).find(item => item.game && item.game.id === "labels-crowded" && item.visible)
        check(tipAnchor, "the crowded tile has no label tip")
        const tipLoader = child(tipAnchor, "desktopGameLabelTipLoader")
        tipAnchor.shown = true
        check(tipLoader.active, "the label tip did not open")
        const tile = tipAnchor.parent
        tile.visible = false
        check(!tipAnchor.shown && !tipLoader.active, "the label tip stayed open after its tile was hidden")
        tile.visible = true
        check(!tipAnchor.shown, "the label tip reopened without a new hover")
        tipAnchor.shown = true
        tile.enabled = false
        check(!tipAnchor.shown && !tipLoader.active, "the label tip stayed open after its tile was disabled")
        tile.enabled = true

        // A Popup is reparented into the window overlay. Exercise its route
        // guard with a retained, visible anchor, independently of catalog hide
        // propagation and hover state left behind by a held mouse button.
        const retainedTip = Qt.createQmlObject('import QtQuick; import OpenNOW; DesktopGameLabelTip { width: 132; height: 198 }',
            host, "retainedGameLabelTip")
        retainedTip.game = crowded
        retainedTip.requested = true
        retainedTip.shown = true
        const retainedLoader = child(retainedTip, "desktopGameLabelTipLoader")
        check(retainedLoader.active, "the retained tip did not open on a catalog route")
        AppController.navigate("inserting")
        check(!retainedTip.anchorUsable && !retainedTip.shown && !retainedLoader.active,
            "the retained tip stayed open while requesting a session")
        AppController.navigate("stream")
        retainedTip.shown = true
        check(!retainedLoader.active, "a stale hover reopened the tip above the stream")
        retainedTip.shown = false
        retainedTip.destroy()
        AppController.navigate("library")

        const upscaledLabels = labelsFor(host, "labels-dlss")
        check(upscaledLabels, "the DLSS tile labels were not rendered")
        const dlssMark = child(upscaledLabels, "desktopGameFeature-dlss")
        check(dlssMark && dlssMark.visible && dlssMark.status === Image.Ready, "game.dlss did not draw the DLSS mark")
        check(upscaledLabels.summary.indexOf("DLSS") >= 0, "DLSS is missing from the accessible description")

        const dlssAnchor = collect(host, "desktopGameLabelTipAnchor", [])
            .find(item => item.game && item.game.id === "labels-dlss" && item.visible)
        check(dlssAnchor, "the DLSS tile has no label tip")
        const dlssRows = () => visibleTexts(host).filter(item => item.text === DesktopTokens.featureTip("dlss"))
        check(dlssRows().length === 0, "the DLSS row is painted before its tip opened")
        const dlssLoader = child(dlssAnchor, "desktopGameLabelTipLoader")
        dlssAnchor.shown = true
        check(dlssLoader.active, "the label tip did not open on the DLSS tile")
        const painted = dlssRows()
        check(painted.length === 1, "the DLSS tile tip does not paint the DLSS row exactly once (found " + painted.length + ")")
        dlssAnchor.shown = false

        const plainLabels = collect(host, "desktopGameLabels", []).find(item => item.game && item.game.id === "labels-plain")
        check(plainLabels && !plainLabels.visible, "a game without catalog labels draws an empty pill")

        check(AppController.showOverlay("desktop-icon-guide"), "the icon guide overlay is not registered")
        const guide = collect(host, "desktopIconGuide", [])[0]
        check(guide && guide.opened, "the icon guide did not open")
        const listed = DesktopTokens.gameFeatureKeys.filter(key => child(guide, "desktopIconGuideFeature-" + key))
        check(JSON.stringify(listed) === JSON.stringify(["rtx", "hdr", "reflex", "dlss"]),
            "the icon guide does not list exactly the backed features")
        const dlssEntry = child(guide, "desktopIconGuideFeature-dlss")
        check(dlssEntry.label === "DLSS" && dlssEntry.detail.indexOf("NVIDIA's DLSS games list") >= 0
            && child(guide, "desktopIconGuideFeature-rtx").detail === "",
            "the icon guide does not say DLSS comes from NVIDIA's list")
        const close = child(guide, "desktopIconGuideClose")
        guide.restoreFocus()
        check(close.activeFocus, "the icon guide did not take focus")
        check(guide.handleKey(Qt.Key_Tab) && close.activeFocus, "Tab left the icon guide")
        check(guide.handleKey(Qt.Key_Backtab) && close.activeFocus, "Shift+Tab left the icon guide")
        check(guide.handleKey(Qt.Key_Escape) && AppController.overlay === "", "Escape did not close the icon guide")
        check(!guide.opened, "the icon guide did not close")
        return true
    }
}

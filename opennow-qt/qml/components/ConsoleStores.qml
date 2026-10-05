pragma Singleton
import QtQuick
import OpenNOW

QtObject {
    function label(store) {
        return DesktopTokens.storeLabel(store)
    }

    function iconUrl(store) {
        return DesktopTokens.storeIconUrl(store)
    }

    function color(store) {
        const colors = {steam: Theme.cartSteam, epic: Theme.cartEpic, ubisoft: Theme.cartUbisoft,
            xbox: Theme.cartXbox, gog: Theme.cartGog, battlenet: Theme.cartBattlenet}
        return colors[DesktopTokens.storeKey(store)] || "#252A35"
    }

    function ownership(variant) {
        if (!variant)
            return "unknown"
        if (["MANUAL", "PLATFORM_SYNC"].indexOf(variant.libraryStatus) >= 0)
            return "owned"
        if (variant.libraryStatus === "NOT_OWNED")
            return "not-owned"
        if (variant.libraryStatus === undefined && variant.inLibrary === true)
            return "owned"
        return "unknown"
    }

    function ownershipLabel(variant) {
        const state = ownership(variant)
        return state === "owned" ? qsTr("Owned")
            : state === "not-owned" ? qsTr("Not owned") : qsTr("Ownership unconfirmed")
    }

    function stores(game) {
        if (!game)
            return []
        const variants = game.variants || []
        const values = variants.length
            ? variants.map(variant => String(variant.store || ""))
            : (game.availableStores || []).map(store => String(store || ""))
        const unique = []
        for (const value of values) {
            const store = value.trim()
            if (store.length && unique.indexOf(store) < 0)
                unique.push(store)
        }
        return unique
    }

    function ownedStore(game) {
        if (!game)
            return ""
        const variants = game.variants || []
        const selected = variants[Number(game.selectedVariantIndex || 0)]
        if (ownership(selected) === "owned")
            return String(selected.store || "")
        const owned = variants.find(variant => ownership(variant) === "owned")
        return owned ? String(owned.store || "") : ""
    }

    function primaryStore(game) {
        return ownedStore(game) || stores(game)[0] || ""
    }
}

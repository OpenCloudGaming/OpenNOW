import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import OpenNOW

DesktopGameDetailsDialog {
    id: root
    game: ShellStore.selectedGame
    signal playRequested()
    signal variantSelected(int index)
    objectName: "desktopGameModal"
    badgeText: root.ownershipText
    metaText: [root.game && (root.game.publisherName || root.game.publisher) || "", root.lastPlayedText, root.game && root.game.hoursPlayed ? qsTr("%1 h").arg(root.game.hoursPlayed) : ""].filter(Boolean).join(" · ")
    initialFocusItem: primaryAction

    readonly property var streamSettings: ShellStore.settings || ({})
    readonly property var selectedVariant: {
        const game = root.game
        if (!game)
            return null
        const variants = game.variants || []
        if (!variants.length)
            return null
        const index = Number(game.selectedVariantIndex || 0)
        return index >= 0 && index < variants.length ? variants[index] : null
    }
    readonly property bool gameAvailable: {
        const game = root.game
        if (!game)
            return false
        return ShellStore.selectedLaunchDecision.status === "ready"
    }
    readonly property bool hasRtx: {
        const game = root.game
        if (!game)
            return false
        const parts = [String(game.title || "")]
        const lists = [game.genres, game.nvidiaTech, game.featureLabels]
        for (let i = 0; i < lists.length; ++i) {
            const list = lists[i] || []
            for (let j = 0; j < list.length; ++j)
                parts.push(String(list[j]))
        }
        const skuTags = game.catalogSkuStrings && game.catalogSkuStrings.SKU_BASED_TAG
        if (skuTags) {
            for (let k = 0; k < skuTags.length; ++k)
                parts.push(String(skuTags[k]))
        }
        return parts.join(" ").toLowerCase().indexOf("rtx") >= 0
    }
    readonly property string lastPlayedText: {
        const game = root.game
        if (!game)
            return qsTr("—")
        if (game.lastPlayedLabel)
            return String(game.lastPlayedLabel)
        const raw = String(game.lastPlayed || (root.selectedVariant && root.selectedVariant.lastPlayedDate) || "")
        if (!raw)
            return qsTr("Not played yet")
        return DesktopTokens.relativeLastPlayed(raw, Date.now()) || qsTr("—")
    }
    readonly property string storesText: {
        const game = root.game
        if (!game)
            return qsTr("—")
        const fromStores = game.availableStores || []
        const stores = fromStores.length
            ? fromStores
            : (game.variants || []).map(variant => variant && variant.store).filter(Boolean)
        return stores.length ? stores.join(" · ") : qsTr("—")
    }
    readonly property bool isOwned: root.selectedVariant
        && ["MANUAL", "PLATFORM_SYNC"].indexOf(root.selectedVariant.libraryStatus) >= 0
    readonly property string ownershipText: {
        const game = root.game
        const variant = root.selectedVariant
        const store = variant && variant.store
            ? String(variant.store)
            : ((game && game.availableStores && game.availableStores[0]) || "")
        if (store && root.isOwned)
            return qsTr("OWNED ON %1").arg(store.toUpperCase())
        if (store)
            return store.toUpperCase()
        return root.isOwned ? qsTr("IN LIBRARY") : qsTr("NOT OWNED")
    }
    readonly property string membershipText: {
        const sub = ShellStore.subscription
        if (sub && sub.membershipTier)
            return String(sub.membershipTier).toUpperCase()
        const user = ShellStore.authSession && ShellStore.authSession.user
        if (user && user.membershipTier)
            return String(user.membershipTier).toUpperCase()
        return ""
    }
    readonly property string resolutionText: {
        const raw = String(root.streamSettings.resolution || "")
        if (raw.indexOf("x") > 0) {
            const height = Number(raw.split("x")[1])
            if (height >= 2160)
                return "4K"
            if (height > 0)
                return height + "p"
        }
        return raw || qsTr("Auto")
    }
    readonly property string fpsText: {
        const fps = Number(root.streamSettings.fps || 0)
        return fps > 0 ? qsTr("%1 fps").arg(fps) : qsTr("Auto")
    }
    readonly property string codecText: {
        const codec = String(root.streamSettings.codec || "")
        if (!codec || codec.toLowerCase() === "auto")
            return qsTr("Auto")
        return codec.toUpperCase()
    }
    readonly property string regionText: {
        const region = String(root.streamSettings.region || "")
        return region ? region.toUpperCase() : qsTr("AUTOMATIC REGION")
    }
    readonly property string friendsNote: {
        const caps = ShellStore.socialCapabilities || ({})
        if (!caps.friendsAvailable && caps.reason)
            return String(caps.reason)
        return qsTr("Friends activity is not available from GeForce NOW")
    }
    readonly property var badgeLabels: {
        const game = root.game
        const labels = []
        const seen = {}
        function add(label) {
            const text = String(label || "").trim()
            const key = text.toUpperCase()
            if (!text || seen[key])
                return
            seen[key] = true
            labels.push(text)
        }
        if (!game)
            return labels
        if (root.gameAvailable)
            add(qsTr("READY TO PLAY"))
        else if (game.playabilityState)
            add(String(game.playabilityState).replace(/_/g, " "))
        const playType = String(game.playType || "").replace(/_/g, " ")
        if (playType && playType.toUpperCase() !== "READY TO PLAY")
            add(playType)
        const controls = game.supportedControls || []
        for (let i = 0; i < controls.length; ++i) {
            const control = String(controls[i] || "").toUpperCase()
            if (control === "GAMEPAD")
                add(qsTr("CONTROLLER"))
            else if (control === "KEYBOARD_MOUSE" || control === "KEYBOARD AND MOUSE")
                add(qsTr("KEYBOARD"))
            else if (control)
                add(control.replace(/_/g, " "))
        }
        if (root.hasRtx)
            add("RTX")
        if (game.membershipTierLabel)
            add(String(game.membershipTierLabel))
        return labels
    }
    readonly property var factItems: {
        const game = root.game || ({})
        const items = [{l: qsTr("LAST PLAYED"), v: root.lastPlayedText}]
        if (game.hoursPlayed)
            items.push({l: qsTr("HOURS PLAYED"), v: qsTr("%1 h").arg(game.hoursPlayed)})
        if (game.sessionCount)
            items.push({l: qsTr("SESSIONS"), v: String(game.sessionCount)})
        items.push({l: qsTr("STORES"), v: root.storesText})
        items.push({l: qsTr("AVAILABLE"), v: root.gameAvailable ? qsTr("Yes") : qsTr("No")})
        return items
    }
    readonly property var streamRows: [
        {l: qsTr("Resolution"), v: root.resolutionText},
        {l: qsTr("Frame rate"), v: root.fpsText},
        {l: qsTr("Codec"), v: root.codecText}
    ]

    function tune() { AppController.navigate("settings-streaming") }
    readonly property var summaryCards: [
        {glyph:"monitor", title:resolutionText + " · " + fpsText, detail:codecText + " · " + String(streamSettings.colorQuality || "8bit_420").replace("_", " ")},
        {glyph:"globe", title:regionLabel(), detail:qsTr("Region selected at launch")},
        {glyph:"clock", title:membershipText || qsTr("Membership"), detail:ShellStore.subscription && ShellStore.subscription.remainingHours !== undefined
            ? qsTr("%1 h remaining").arg(Math.max(0, Number(ShellStore.subscription.remainingHours)).toFixed(1)) : qsTr("Entitlements checked at launch")}
    ]
    function regionLabel() {
        const value = String(streamSettings.region || "")
        const regions = ShellStore.regions || []
        for (const region of regions)
            if (region.url === value || region.name === value) return String(region.name)
        return value ? qsTr("Selected region") : qsTr("Automatic region")
    }
    Column {
        objectName: "gameDetailsReadiness"
        width: parent.width
        spacing: DesktopTokens.px(8)
        Text {
            id: readinessNoticeLabel
            objectName: "catalogReadinessNotice"
            width: parent.width
            text: I18n.source(ShellStore.cloudMutationMessage || ShellStore.selectedLaunchDecision.message || ShellStore.readinessNotice(root.game), I18n.revision)
            visible: text !== ""
            wrapMode: Text.WordWrap
            color: ShellStore.cloudMutationState === "unconfirmed" ? (Theme.lightMode ? Qt.darker(DesktopTokens.danger, 2) : DesktopTokens.danger) : Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
        }
        visible: storeVariants.count > 1 || readinessNoticeLabel.text !== ""
        Text {
            text: qsTr("PLATFORM")
            visible: storeVariants.count > 1
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.smallSize
            font.weight: Font.Bold
        }
        Flow {
            visible: storeVariants.count > 1
            width: parent.width
            spacing: DesktopTokens.px(8)
            Repeater {
                id: storeVariants
                model: root.game ? root.game.variants || [] : []
                DesktopButton {
                    id: platformButton
                    required property var modelData
                    required property int index
                    objectName: "desktopStoreVariant" + index
                    text: String(modelData.store || qsTr("Unknown"))
                    height: DesktopTokens.px(36)
                    leftPadding: DesktopTokens.px(14)
                    rightPadding: DesktopTokens.px(14)
                    font.pixelSize: DesktopTokens.captionSize
                    implicitWidth: platformContents.implicitWidth + leftPadding + rightPadding
                    checkable: true
                    autoExclusive: true
                    checked: root.game ? index === Number(root.game.selectedVariantIndex || 0) : false
                    primary: checked
                    Accessible.description: ["MANUAL", "PLATFORM_SYNC"].indexOf(modelData.libraryStatus) >= 0
                        ? qsTr("Owned") : modelData.libraryStatus === "NOT_OWNED" ? qsTr("Not owned") : qsTr("Ownership unconfirmed")
                    onClicked: root.variantSelected(index)
                    Keys.onReturnPressed: root.variantSelected(index)
                    Keys.onEnterPressed: root.variantSelected(index)
                    contentItem: Row {
                        id: platformContents
                        spacing: DesktopTokens.px(8)
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: DesktopTokens.px(26)
                            height: width
                            radius: DesktopTokens.px(5)
                            visible: platformLogo.source.toString() !== ""
                            color: platformButton.checked || Theme.lightMode ? "#202634" : "transparent"
                            Image {
                                id: platformLogo
                                objectName: "desktopStoreLogo" + platformButton.index
                                anchors.centerIn: parent
                                width: DesktopTokens.px(20)
                                height: width
                                source: DesktopTokens.storeIconUrl(platformButton.modelData.store)
                                sourceSize: Qt.size(width * Screen.devicePixelRatio, height * Screen.devicePixelRatio)
                                fillMode: Image.PreserveAspectFit
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: platformButton.text
                            font: platformButton.font
                            color: platformButton.checked ? "#0A0D14" : Theme.label
                        }
                    }
                }
            }
        }
    }
    RowLayout {
        id: actionRow
        objectName: "gameDetailsPrimaryActions"
        width: parent.width
        spacing: DesktopTokens.px(10)
        DesktopButton {
            id: primaryAction
            objectName: "desktopGamePlay"
            Layout.fillWidth: true; Layout.minimumWidth: 0; Layout.preferredHeight: DesktopTokens.px(52)
            font.pixelSize: DesktopTokens.captionSize
            leftPadding: DesktopTokens.px(14); rightPadding: DesktopTokens.px(14)
            primary: true; glyph: "desktop-play.svg"; text: ShellStore.selectedGameActionLabel(); shortcutText: qsTr("ENTER"); shortcutSequence: "Enter"
            enabled: root.game !== null && !ShellStore.cloudMutationBusy && ShellStore.launchInspectRequestId === ""
            onClicked: root.playRequested()
        }
        DesktopButton {
            Layout.preferredWidth: DesktopTokens.px(52); Layout.preferredHeight: DesktopTokens.px(52)
            themedGlyph: "star"; leftPadding: 0; rightPadding: 0
            Accessible.name: root.game && ShellStore.isCloudFavorite(root.game) ? qsTr("Remove from GeForce NOW favorites") : qsTr("Add to GeForce NOW favorites")
            ToolTip.visible: hovered; ToolTip.text: Accessible.name
            enabled: ShellStore.signedIn && !ShellStore.cloudMutationBusy
            onClicked: if (root.game) ShellStore.toggleCloudFavorite(root.game)
        }
        DesktopButton {
            Layout.preferredWidth: DesktopTokens.px(52); Layout.preferredHeight: DesktopTokens.px(52)
            themedGlyph: "folder"; leftPadding: 0; rightPadding: 0
            Accessible.name: qsTr("Collections")
            ToolTip.visible: hovered; ToolTip.text: Accessible.name
            onClicked: collectionMenu.popup()
            Menu {
                id: collectionMenu
                MenuItem { text: qsTr("Pin to Home"); checkable: true; checked: root.game && ShellStore.isFavorite(root.game); onTriggered: if (root.game) ShellStore.toggleFavorite(root.game) }
            }
        }
        DesktopButton {
            Layout.preferredWidth: DesktopTokens.px(52); Layout.preferredHeight: DesktopTokens.px(52)
            themedGlyph: "more"; leftPadding: 0; rightPadding: 0
            Accessible.name: qsTr("More game actions")
            onClicked: moreMenu.popup()
            Menu {
                id: moreMenu
                MenuItem { text: qsTr("Stream settings"); onTriggered: root.tune() }
                MenuItem { text: qsTr("Close details"); onTriggered: root.closeRequested() }
            }
        }
    }
    CloudLibraryActions {
        width: parent.width
        game: root.game
        showFavorites: false
        showStatus: false
    }
    GridLayout {
        id: summaryGrid
        objectName: "gameDetailsSummary"
        width: parent.width
        columns: width < DesktopTokens.px(620) ? 2 : 4
        uniformCellWidths: columns === 2
        columnSpacing: DesktopTokens.px(10); rowSpacing: DesktopTokens.px(10)
        Repeater {
            model: root.summaryCards
            delegate: DesktopGameDetailsSummaryCard {
                required property var modelData
                glyph: modelData.glyph
                title: modelData.title
                detail: modelData.detail
            }
        }
        DesktopButton {
            objectName: "gameDetailsTune"
            Layout.fillWidth: summaryGrid.columns === 2
            Layout.preferredWidth: Math.max(implicitWidth, DesktopTokens.px(68)); Layout.preferredHeight: DesktopTokens.px(68)
            font.pixelSize: DesktopTokens.captionSize
            text: qsTr("Tune"); themedGlyph: "sliders"; leftPadding: DesktopTokens.px(6); rightPadding: DesktopTokens.px(6)
            onClicked: root.tune()
        }
    }
    Keys.onReturnPressed: if (primaryAction.enabled) root.playRequested()
}

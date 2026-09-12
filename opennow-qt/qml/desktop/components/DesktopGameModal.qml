import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

FocusScope {
    id: root
    property var game: ShellStore.selectedGame
    signal closeRequested()
    signal playRequested()
    objectName: "desktopGameModal"
    property bool opened: false
    visible: reveal.present
    enabled: opened
    focus: opened
    onOpenedChanged: if (opened) Qt.callLater(() => { if (root.opened) primaryAction.forceActiveFocus() })
    MotionProgress { id: reveal; objectName: "gameDetailsMotion"; shown: root.opened }

    readonly property int gutter: DesktopTokens.px(32)
    readonly property int dialogWidth: Math.min(DesktopTokens.px(760), Math.max(0, width - gutter * 2))
    readonly property int dialogHeight: Math.min(DesktopTokens.px(550), Math.max(0, height - gutter * 2))
    readonly property var streamSettings: ShellStore.settings || ({})
    readonly property var selectedVariant: {
        const game = root.game
        if (!game)
            return null
        const variants = game.variants || []
        if (!variants.length)
            return null
        const index = Math.max(0, Math.min(variants.length - 1, Number(game.selectedVariantIndex || 0)))
        return variants[index]
    }
    readonly property bool gameAvailable: {
        const game = root.game
        if (!game)
            return false
        if (game.isAvailable !== undefined && game.isAvailable !== null)
            return Boolean(game.isAvailable)
        const state = String(game.playabilityState || "").toUpperCase()
        if (state === "PLAYABLE" || state === "AVAILABLE")
            return true
        if (state.indexOf("UNPLAYABLE") >= 0 || state === "NOT_PLAYABLE" || state === "NOT_AVAILABLE")
            return false
        return Boolean(game.isInLibrary)
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
        ? Boolean(root.selectedVariant.inLibrary)
        : Boolean(root.game && root.game.isInLibrary)
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
        if (root.game && root.game.membershipTierLabel)
            return String(root.game.membershipTierLabel).toUpperCase()
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
    Rectangle {
        anchors.fill: parent; color: "#A6040D10"; opacity: reveal.progress
        MouseArea {
            anchors.fill: parent; acceptedButtons: Qt.AllButtons
            hoverEnabled: true; preventStealing: true
            onClicked: root.closeRequested()
            onWheel: wheel => wheel.accepted = true
        }
    }
    Rectangle {
        id: dialog
        objectName: "gameDetailsDialog"
        opacity: reveal.progress
        scale: reveal.zoom
        transformOrigin: Item.Center
        anchors.centerIn: parent
        width: root.dialogWidth; height: root.dialogHeight
        radius: 24; color: Theme.shell; border.width: 1; border.color: Theme.seam
        clip: true
        // Swallow blank-space clicks inside the modal, never activate its scrim.
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: wheel => wheel.accepted = true }
        Flickable {
            anchors.fill: parent
            contentWidth: width; contentHeight: detailsColumn.implicitHeight
            clip: true; boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            Column {
                id: detailsColumn
                width: parent.width
                Item {
                    width: parent.width; height: Math.min(360 * DesktopTokens.uiScale, root.dialogHeight * 0.65)
                    RoundedArtwork {
                        anchors.fill: parent; artwork: DesktopTokens.artworkUrl(root.game, true)
                        cornerRadius: 24; scrimStart: 0.1; fallbackColor: Theme.shell
                    }
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            GradientStop { position: 0.25; color: "transparent" }
                            GradientStop { position: 1; color: Theme.shell }
                        }
                    }
                    Column {
                        x: 24; anchors.bottom: parent.bottom; anchors.bottomMargin: 20
                        width: parent.width - 48; spacing: 8
                        Text {
                            width: parent.width; text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game")
                            color: Theme.label; font.family: Theme.displayFont
                            font.pixelSize: DesktopTokens.px(34); font.weight: Font.Black
                            wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                        }
                        Flow {
                            width: parent.width; spacing: 10
                            Rectangle {
                                width: ownedText.implicitWidth + 20; height: 26; radius: 13; color: DesktopTokens.raisedStrong
                                Text { id: ownedText; anchors.centerIn: parent; text: root.ownershipText; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold }
                            }
                            Text {
                                text: [root.game && (root.game.publisherName || root.game.publisher) || "", root.lastPlayedText, root.game && root.game.hoursPlayed ? qsTr("%1 h").arg(root.game.hoursPlayed) : ""].filter(Boolean).join(" · ")
                                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                            }
                        }
                    }
                }
                Item {
                    width: parent.width; height: actionRow.height + 24
                    RowLayout {
                        id: actionRow
                        x: 24; y: 4; width: parent.width - 48; spacing: 10
                        DesktopButton {
                            id: primaryAction
                            Layout.fillWidth: true; Layout.preferredHeight: 52
                            primary: true; glyph: "desktop-play.svg"; text: qsTr("Play"); shortcutText: qsTr("ENTER"); shortcutSequence: "Enter"
                            enabled: root.game !== null && root.gameAvailable
                            onClicked: root.playRequested()
                        }
                        DesktopButton {
                            Layout.preferredWidth: 52; Layout.preferredHeight: 52
                            themedGlyph: "star"; leftPadding: 0; rightPadding: 0
                            Accessible.name: root.game && ShellStore.isFavorite(root.game) ? qsTr("Remove favourite") : qsTr("Add favourite")
                            ToolTip.visible: hovered; ToolTip.text: Accessible.name
                            onClicked: if (root.game) ShellStore.toggleFavorite(root.game)
                        }
                        DesktopButton {
                            Layout.preferredWidth: 52; Layout.preferredHeight: 52
                            themedGlyph: "folder"; leftPadding: 0; rightPadding: 0
                            Accessible.name: qsTr("Collections")
                            ToolTip.visible: hovered; ToolTip.text: Accessible.name
                            onClicked: collectionMenu.popup()
                            Menu {
                                id: collectionMenu
                                MenuItem { text: qsTr("Favourites"); checkable: true; checked: root.game && ShellStore.isFavorite(root.game); onTriggered: if (root.game) ShellStore.toggleFavorite(root.game) }
                            }
                        }
                        DesktopButton {
                            Layout.preferredWidth: 52; Layout.preferredHeight: 52
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
                }
                Item {
                    width: parent.width; height: summaryGrid.implicitHeight + 24
                    GridLayout {
                        id: summaryGrid
                        x: 24; width: parent.width - 48
                        columns: width < 620 ? 2 : 4
                        columnSpacing: 10; rowSpacing: 10
                        Repeater {
                            model: root.summaryCards
                            delegate: Rectangle {
                                required property var modelData
                                Layout.fillWidth: true; Layout.preferredHeight: 68
                                radius: 16; color: DesktopTokens.raised
                                RowLayout {
                                    anchors.fill: parent; anchors.margins: 12; spacing: 12
                                    Rectangle {
                                        Layout.preferredWidth: 36; Layout.preferredHeight: 36
                                        radius: 11; color: DesktopTokens.raised
                                        // Paper's accent icons sit on their own tile. Never use
                                        // the fixed dark-ink settings SVGs on a dark surface.
                                        DesktopSettingsIcon {
                                            anchors.centerIn: parent; width: 18; height: 18
                                            glyph: modelData.glyph
                                            ink: Theme.lightMode ? Theme.label : Theme.focus
                                        }
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true; spacing: 2
                                        Text { Layout.fillWidth: true; text: modelData.title; elide: Text.ElideRight; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold }
                                        Text { Layout.fillWidth: true; text: modelData.detail; elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.smallSize }
                                    }
                                }
                            }
                        }
                        DesktopButton {
                            Layout.preferredWidth: 68; Layout.preferredHeight: 68
                            text: qsTr("Tune"); themedGlyph: "sliders"; leftPadding: 6; rightPadding: 6
                            onClicked: root.tune()
                        }
                    }
                }
            }
        }
        DesktopButton {
            anchors.right: parent.right; anchors.rightMargin: 20
            anchors.top: parent.top; anchors.topMargin: 20
            width: 36; height: 36; themedGlyph: "close"; leftPadding: 0; rightPadding: 0
            Accessible.name: qsTr("Close details")
            onClicked: root.closeRequested()
        }
    }
    Keys.onEscapePressed: root.closeRequested()
    Keys.onReturnPressed: if (primaryAction.enabled) root.playRequested()
}

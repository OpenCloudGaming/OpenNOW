import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    property var game: ShellStore.selectedGame
    signal closeRequested()
    signal playRequested()
    signal variantSelected(int index)
    objectName: "desktopGameModal"
    property bool opened: false
    visible: reveal.present
    enabled: opened
    focus: opened
    onOpenedChanged: if (opened) Qt.callLater(() => { if (root.opened) primaryAction.forceActiveFocus() })
    MotionProgress { id: reveal; objectName: "gameDetailsMotion"; shown: root.opened }

    readonly property int gutter: DesktopTokens.px(32)
    readonly property int dialogWidth: Math.min(DesktopTokens.px(760), Math.max(0, width - gutter * 2))
    readonly property int maximumDialogHeight: Math.max(0, height - gutter * 2)
    readonly property int dialogHeight: Math.min(maximumDialogHeight, Math.ceil(detailsColumn.implicitHeight))
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

    function revealFocusedControl() {
        if (!root.opened || !root.Window.window)
            return
        const item = root.Window.window.activeFocusItem
        let ancestor = item
        while (ancestor && ancestor !== detailsColumn)
            ancestor = ancestor.parent
        if (!ancestor)
            return
        const top = item.mapToItem(detailsFlick.contentItem, 0, 0).y
        const margin = DesktopTokens.px(8)
        const maximumY = Math.max(0, detailsFlick.contentHeight - detailsFlick.height)
        if (top < detailsFlick.contentY + margin)
            detailsFlick.contentY = Math.max(0, top - margin)
        else if (top + item.height > detailsFlick.contentY + detailsFlick.height - margin)
            detailsFlick.contentY = Math.min(maximumY, top + item.height + margin - detailsFlick.height)
    }
    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() { Qt.callLater(root.revealFocusedControl) }
    }

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
        radius: DesktopTokens.px(24); color: Theme.shell; border.width: 1; border.color: Theme.seam
        // Swallow blank-space clicks inside the modal, never activate its scrim.
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: wheel => wheel.accepted = true }
        Flickable {
            id: detailsFlick
            objectName: "gameDetailsScroll"
            anchors.fill: parent
            contentWidth: width; contentHeight: detailsColumn.implicitHeight
            clip: true; boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick
            onHeightChanged: Qt.callLater(root.revealFocusedControl)
            onContentHeightChanged: Qt.callLater(root.revealFocusedControl)
            ScrollBar.vertical: ScrollBar {
                policy: detailsFlick.contentHeight > detailsFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            }
            Column {
                id: detailsColumn
                width: parent.width
                Item {
                    width: parent.width
                    height: Math.max(headerInfo.implicitHeight + DesktopTokens.px(48),
                        Math.min(DesktopTokens.px(280), root.maximumDialogHeight * 0.38))
                    RoundedArtwork {
                        anchors.fill: parent; artwork: DesktopTokens.artworkUrl(root.game, true)
                        cornerRadius: DesktopTokens.px(24); scrimStart: 0.1; fallbackColor: Theme.shell
                    }
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            GradientStop { position: 0.25; color: "transparent" }
                            GradientStop { position: 1; color: Theme.shell }
                        }
                    }
                    Column {
                        id: headerInfo
                        x: DesktopTokens.px(24); anchors.bottom: parent.bottom; anchors.bottomMargin: DesktopTokens.px(24)
                        width: parent.width - DesktopTokens.px(48); spacing: DesktopTokens.px(8)
                        Text {
                            width: parent.width; text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game")
                            color: Theme.label; font.family: Theme.displayFont
                            font.pixelSize: DesktopTokens.px(34); font.weight: Font.Black
                            wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                        }
                        Flow {
                            width: parent.width; spacing: DesktopTokens.px(10)
                            Rectangle {
                                width: Math.min(parent.width, ownedText.implicitWidth + DesktopTokens.px(20))
                                height: DesktopTokens.px(26); radius: DesktopTokens.px(13); color: DesktopTokens.raisedStrong
                                Text { id: ownedText; anchors.centerIn: parent; width: parent.width - DesktopTokens.px(20); elide: Text.ElideRight; text: root.ownershipText; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold }
                            }
                            Text {
                                width: Math.min(implicitWidth, parent.width)
                                wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                                text: [root.game && (root.game.publisherName || root.game.publisher) || "", root.lastPlayedText, root.game && root.game.hoursPlayed ? qsTr("%1 h").arg(root.game.hoursPlayed) : ""].filter(Boolean).join(" · ")
                                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                            }
                        }
                    }
                }
                Item {
                    width: parent.width
                    height: bodyContent.implicitHeight + DesktopTokens.px(24)
                    Column {
                        id: bodyContent
                        objectName: "gameDetailsBody"
                        x: DesktopTokens.px(24)
                        width: parent.width - DesktopTokens.px(48)
                        spacing: DesktopTokens.px(20)
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
                                delegate: Rectangle {
                                    required property var modelData
                                    objectName: "gameDetailsSummaryCard"
                                    Layout.fillWidth: true; Layout.minimumWidth: 0; Layout.preferredHeight: DesktopTokens.px(68)
                                    Layout.preferredWidth: DesktopTokens.px(180)
                                    radius: DesktopTokens.px(16); color: DesktopTokens.raised
                                    RowLayout {
                                        anchors.fill: parent; anchors.margins: DesktopTokens.px(12); spacing: DesktopTokens.px(12)
                                        Rectangle {
                                            Layout.preferredWidth: DesktopTokens.px(36); Layout.preferredHeight: DesktopTokens.px(36)
                                            radius: DesktopTokens.px(11); color: DesktopTokens.raised
                                            // Paper's accent icons sit on their own tile. Never use
                                            // the fixed dark-ink settings SVGs on a dark surface.
                                            DesktopSettingsIcon {
                                                anchors.centerIn: parent; width: DesktopTokens.px(18); height: DesktopTokens.px(18)
                                                glyph: modelData.glyph
                                                ink: Theme.lightMode ? Theme.label : Theme.focus
                                            }
                                        }
                                        ColumnLayout {
                                            Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: DesktopTokens.px(2)
                                            Text { Layout.fillWidth: true; Layout.minimumWidth: 0; text: modelData.title; elide: Text.ElideRight; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold }
                                            Text { Layout.fillWidth: true; Layout.minimumWidth: 0; text: modelData.detail; elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.smallSize }
                                        }
                                    }
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
                    }
                }
            }
        }
        DesktopButton {
            objectName: "gameDetailsClose"
            anchors.right: parent.right; anchors.rightMargin: -width / 2
            anchors.top: parent.top; anchors.topMargin: -height / 2
            width: DesktopTokens.px(36); height: DesktopTokens.px(36); themedGlyph: "close"; leftPadding: 0; rightPadding: 0
            cornerRadius: width / 2
            Accessible.name: qsTr("Close details")
            onClicked: root.closeRequested()
        }
    }
    Keys.onEscapePressed: root.closeRequested()
    Keys.onReturnPressed: if (primaryAction.enabled) root.playRequested()
}

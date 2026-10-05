import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property var previewGame: null
    property bool initialPlatformOpen: false
    property bool libraryOptionsOpen: false
    property bool platformSheetOpen: false
    property Item focusOrigin: play
    readonly property var game: previewGame || ShellStore.selectedGame || ({ title: qsTr("Choose a game"), availableStores: [], variants: [] })
    readonly property string artwork: game.heroImageUrl || game.imageUrl || ""
    readonly property bool canLaunch: Boolean(previewGame) || !ShellStore.signedIn || ShellStore.selectedLaunchAppId() !== ""
    readonly property var variants: game.variants || []
    readonly property int selectedVariantIndex: Math.max(-1, Math.min(variants.length - 1, Number(game.selectedVariantIndex ?? 0)))
    readonly property var selectedVariant: selectedVariantIndex >= 0 ? variants[selectedVariantIndex] : null
    readonly property bool variantOwned: Boolean(selectedVariant)
        && ["MANUAL", "PLATFORM_SYNC"].indexOf(selectedVariant.libraryStatus) >= 0
    readonly property bool inlineVariants: variants.length <= 4
    readonly property var chipEntries: inlineVariants
        ? variants.map((variant, index) => ({variant: variant, index: index}))
        : (selectedVariant ? [{variant: selectedVariant, index: selectedVariantIndex}] : [])
    readonly property bool sheetOpen: libraryOptionsOpen || platformSheetOpen || ownershipSheet.opened
    readonly property var libraryOptions: {
        const busy = ShellStore.cloudMutationBusy
        const options = []
        const variant = root.selectedVariant
        if (variant && variant.libraryStatus === "NOT_OWNED" && ShellStore.selectedLaunchDecision.status !== "ownership_required")
            options.push({value: "own", label: qsTr("I own this game"), disabled: !ShellStore.signedIn || busy,
                detail: qsTr("Adds the %1 version to your GeForce NOW library").arg(ConsoleStores.label(variant.store))})
        options.push({value: "favorite", disabled: !ShellStore.signedIn || busy,
            label: ShellStore.isCloudFavorite(root.game) ? qsTr("Remove from GeForce NOW favorites") : qsTr("Add to GeForce NOW favorites")})
        if (root.variantOwned) {
            options.push({value: "remove", disabled: busy,
                label: qsTr("Remove %1 ownership").arg(ConsoleStores.label(variant.store))})
            if (variant.librarySelected !== true)
                options.push({value: "select", label: qsTr("Use this store version"), disabled: busy})
        }
        options.push({value: "refresh", label: qsTr("Refresh status"), disabled: !ShellStore.signedIn || busy})
        options.push({value: "accounts", label: qsTr("Game accounts"), detail: qsTr("Link stores and sync your library")})
        return options
    }

    Keys.onPressed: event => {
        if (root.sheetOpen)
            return
        if (event.key === Qt.Key_Y) {
            if (!event.isAutoRepeat)
                ShellStore.toggleFavorite(root.game)
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (!event.isAutoRepeat && play.enabled)
                play.click()
        } else if (event.key === Qt.Key_Menu) {
            if (!event.isAutoRepeat)
                root.openLibraryOptions()
        } else return
        event.accepted = true
    }

    function streamQualityLabel() {
        const resolution = String(ShellStore.settings.resolution || "")
        if (!resolution)
            return "—"
        const parts = resolution.split("x")
        const height = parts.length === 2 ? Number(parts[1]) : 1440
        const named = height >= 2160 ? "4K" : height + "p"
        const fps = Number(ShellStore.settings.fps || 0)
        return fps > 0 ? qsTr("%1 · %2 FPS").arg(named).arg(fps) : named
    }

    function codecLabel() {
        const codec = String(ShellStore.settings.codec || "auto").toUpperCase()
        const quality = String(ShellStore.settings.colorQuality || "")
        const bitDepth = quality.indexOf("10bit") === 0 ? qsTr("10-bit") : qsTr("8-bit")
        const chroma = quality.indexOf("444") >= 0 ? "4:4:4" : "4:2:0"
        return qsTr("%1 · %2 · %3").arg(codec).arg(bitDepth).arg(chroma)
    }

    function regionLabel() {
        const selected = String(ShellStore.selectedRegion || "")
        if (selected.length)
            return selected
        return qsTr("Automatic region")
    }

    function selectVariant(index) {
        if (!previewGame) {
            ShellStore.selectGameVariant(index)
            return
        }
        const nextGame = Object.assign({}, previewGame)
        nextGame.selectedVariantIndex = index
        previewGame = nextGame
    }

    function focusChips() {
        const position = root.inlineVariants ? root.selectedVariantIndex : 0
        const chip = chipRepeater.itemAt(Math.max(0, Math.min(chipRepeater.count - 1, position)))
        if (chip)
            chip.forceActiveFocus()
        else if (moreStores.visible)
            moreStores.forceActiveFocus()
        else if (play.enabled)
            play.forceActiveFocus()
        else
            favoriteButton.forceActiveFocus()
    }

    function focusChip(position) {
        if (position >= chipRepeater.count) {
            if (moreStores.visible)
                moreStores.forceActiveFocus()
            return
        }
        const chip = chipRepeater.itemAt(Math.max(0, position))
        if (chip)
            chip.forceActiveFocus()
    }

    function openLibraryOptions() {
        root.focusOrigin = optionsButton
        root.libraryOptionsOpen = true
    }

    function closeSheets() {
        root.libraryOptionsOpen = false
        root.platformSheetOpen = false
        const origin = root.focusOrigin
        Qt.callLater(() => { if (origin && !root.sheetOpen) origin.forceActiveFocus() })
    }

    function runLibraryOption(value) {
        root.libraryOptionsOpen = false
        if (value === "own")
            ShellStore.requestOwnershipConfirmation("add")
        else if (value === "favorite")
            ShellStore.toggleCloudFavorite(root.game)
        else if (value === "remove")
            ShellStore.requestOwnershipConfirmation("remove")
        else if (value === "select")
            ShellStore.selectPreferredVariant()
        else if (value === "refresh")
            ShellStore.refreshSelectedMetadata()
        else if (value === "accounts") {
            AppController.navigate("game-accounts")
            return
        }
        root.closeSheets()
    }

    Rectangle { anchors.fill: parent; color: Theme.shell }
    ArtworkSource {
        id: heroSource
        sourceUrl: DesktopTokens.decodeArtworkUrl(root.artwork)
        active: root.visible
    }
    Image {
        id: hero
        x: Math.round(parent.width * 0.34)
        width: parent.width - x
        height: parent.height
        source: heroSource.resolvedUrl
        fillMode: Image.PreserveAspectCrop
        sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
        asynchronous: true
        opacity: status === Image.Ready ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.heroDuration } }
    }
    Rectangle {
        x: hero.x
        width: hero.width
        height: parent.height
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Theme.shell }
            GradientStop { position: 0.32; color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.55) }
            GradientStop { position: 0.6; color: "transparent" }
        }
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 280
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 1; color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.9) }
        }
    }

    Column {
        id: heroCopy
        x: 120; y: 196
        width: 860
        spacing: 14
        Text {
            width: parent.width
            text: [root.game.publisherName || root.game.developerName || ""].concat(
                (root.game.genres || []).slice(0, 2).map(genre => DesktopTokens.genreLabel(genre)))
                .filter(Boolean).join(" · ").toUpperCase()
            visible: text !== ""
            elide: Text.ElideRight
            color: Theme.textMuted
            font.family: Theme.monoFont
            font.pixelSize: 16
            font.weight: Font.Bold
            font.letterSpacing: 2.2
        }
        Text {
            width: parent.width
            text: String(root.game.title || "")
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.label
            font.family: Theme.displayFont
            font.pixelSize: 80
            font.weight: Font.Black
            font.letterSpacing: -1.6
            lineHeight: 0.92
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
        Text {
            width: 740
            text: String(root.game.longDescription || root.game.description || root.game.shortDescription || "")
            visible: text !== ""
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            textFormat: Text.PlainText
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: 20
            lineHeight: 1.25
        }
    }

    Column {
        x: 120
        y: Math.max(476, heroCopy.y + heroCopy.height + 56)
        width: 1180
        spacing: 16

        Text {
            visible: root.variants.length > 0
            text: qsTr("PLAY ON")
            color: Theme.textMuted
            font.family: Theme.monoFont
            font.pixelSize: 15
            font.weight: Font.Bold
            font.letterSpacing: 2
        }
        Row {
            id: chipRow
            visible: root.variants.length > 0
            spacing: 14
            Repeater {
                id: chipRepeater
                model: root.chipEntries.length
                onItemRemoved: (index, item) => {
                    if (item.activeFocus)
                        Qt.callLater(root.focusChips)
                }
                ItemDelegate {
                    id: chip
                    required property int index
                    readonly property var entry: root.chipEntries[index] || ({variant: {}, index: -1})
                    readonly property var variant: entry.variant
                    readonly property bool selectedVariant: entry.index === root.selectedVariantIndex
                    readonly property string ownership: ConsoleStores.ownership(variant)
                    objectName: "consolePlatformChip" + entry.index
                    width: chipContent.implicitWidth + 40
                    height: 72
                    padding: 0
                    focusPolicy: Qt.StrongFocus
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: ConsoleStores.label(variant.store)
                    Accessible.description: ConsoleStores.ownershipLabel(variant)
                    Accessible.checked: selectedVariant
                    KeyNavigation.down: play
                    Keys.onLeftPressed: root.focusChip(index - 1)
                    Keys.onRightPressed: root.focusChip(index + 1)
                    Keys.onReturnPressed: event => { if (!event.isAutoRepeat) root.selectVariant(entry.index) }
                    Keys.onEnterPressed: event => { if (!event.isAutoRepeat) root.selectVariant(entry.index) }
                    onClicked: root.selectVariant(entry.index)
                    background: Rectangle {
                        radius: 22
                        color: chip.selectedVariant
                            ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.16)
                            : Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.7)
                        FocusFrame { focused: chip.activeFocus; frameRadius: 22 }
                    }
                    contentItem: Item {
                        Row {
                            id: chipContent
                            x: 14
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 14
                            ConsoleStoreMark {
                                anchors.verticalCenter: parent.verticalCenter
                                store: chip.variant.store || ""
                                markSize: 44
                            }
                            Column {
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 1
                                Text {
                                    text: ConsoleStores.label(chip.variant.store)
                                    color: Theme.label
                                    font.family: Theme.displayFont
                                    font.pixelSize: 20
                                    font.weight: Font.Black
                                }
                                Text {
                                    text: ConsoleStores.ownershipLabel(chip.variant)
                                    color: chip.ownership === "owned"
                                        ? (Theme.lightMode ? Qt.darker(Theme.mint, 2.2) : Theme.mint) : Theme.textMuted
                                    font.family: Theme.bodyFont
                                    font.pixelSize: 16
                                    font.weight: Font.DemiBold
                                }
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: chip.selectedVariant
                                text: "✓"
                                color: Theme.mint
                                font.pixelSize: 22
                                font.weight: Font.Black
                            }
                        }
                    }
                }
            }
            ItemDelegate {
                id: moreStores
                objectName: "consolePlatformMore"
                visible: !root.inlineVariants
                width: moreLabel.implicitWidth + 48
                height: 72
                padding: 0
                focusPolicy: Qt.StrongFocus
                Accessible.role: Accessible.Button
                Accessible.name: moreLabel.text
                KeyNavigation.down: play
                Keys.onLeftPressed: root.focusChip(chipRepeater.count - 1)
                Keys.onReturnPressed: event => { if (!event.isAutoRepeat) clicked() }
                Keys.onEnterPressed: event => { if (!event.isAutoRepeat) clicked() }
                onClicked: {
                    root.focusOrigin = moreStores
                    root.platformSheetOpen = true
                }
                background: Rectangle {
                    radius: 22
                    color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.7)
                    FocusFrame {
                        focused: moreStores.activeFocus || root.platformSheetOpen
                        parked: root.platformSheetOpen
                        frameRadius: 22
                    }
                }
                contentItem: Item {
                    Text {
                        id: moreLabel
                        anchors.centerIn: parent
                        text: qsTr("All %1 stores ›").arg(root.variants.length)
                        color: Theme.label
                        font.family: Theme.displayFont
                        font.pixelSize: 20
                        font.weight: Font.Black
                    }
                }
            }
        }
        Text {
            width: 900
            text: ShellStore.readinessNotice(root.game)
            visible: text !== ""
            wrapMode: Text.WordWrap
            maximumLineCount: 3
            elide: Text.ElideRight
            color: Theme.lightMode ? Qt.darker(Theme.yellow, 2.2) : Theme.yellow
            font.family: Theme.bodyFont
            font.pixelSize: 17
            font.weight: Font.DemiBold
        }
        Item { width: 1; height: 20 }
        Row {
            spacing: 18
            ConsoleActionButton {
                id: play
                objectName: "consolePlayButton"
                text: ShellStore.selectedGameActionLabel()
                glyph: "A"
                primary: true
                enabled: root.canLaunch && !ShellStore.cloudMutationBusy
                KeyNavigation.right: favoriteButton
                Keys.onUpPressed: root.focusChips()
                onClicked: {
                    root.focusOrigin = play
                    if (!root.previewGame)
                        ShellStore.activateSelectedGame()
                }
                Component.onCompleted: forceActiveFocus()
            }
            ConsoleActionButton {
                id: favoriteButton
                objectName: "consoleFavoriteButton"
                text: ShellStore.isFavorite(root.game) ? qsTr("Remove from Home") : qsTr("Pin to Home")
                glyph: "Y"
                KeyNavigation.left: play
                KeyNavigation.right: optionsButton
                Keys.onUpPressed: root.focusChips()
                onClicked: ShellStore.toggleFavorite(root.game)
            }
            ConsoleActionButton {
                id: optionsButton
                objectName: "consoleLibraryOptionsButton"
                text: qsTr("Library options")
                glyph: "MENU"
                currentItem: root.libraryOptionsOpen
                KeyNavigation.left: favoriteButton
                Keys.onUpPressed: root.focusChips()
                onClicked: root.openLibraryOptions()
            }
        }
        Text {
            objectName: "cloudLibraryStatus"
            width: 900
            text: I18n.source(ShellStore.cloudMutationMessage || ShellStore.selectedLaunchDecision.message || "", I18n.revision)
            visible: text !== ""
            wrapMode: Text.WordWrap
            color: ShellStore.cloudMutationState === "unconfirmed" ? Theme.coral : Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: 17
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }
    }

    GlassPanel {
        id: requestsCard
        x: root.width - width - 96
        y: 470
        width: 472
        height: requestsColumn.implicitHeight + 56
        panelRadius: 30
        strong: true
        Column {
            id: requestsColumn
            x: 28; y: 28
            width: parent.width - 56
            spacing: 12
            Item {
                width: parent.width
                height: requestsTitle.implicitHeight
                Text {
                    id: requestsTitle
                    text: qsTr("NEXT LAUNCH REQUESTS")
                    color: Theme.textMuted
                    font.family: Theme.monoFont
                    font.pixelSize: 14
                    font.weight: Font.Bold
                    font.letterSpacing: 1.8
                }
                Text {
                    anchors.right: parent.right
                    text: qsTr("Settings → Streaming")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 15
                    font.weight: Font.Bold
                    Accessible.role: Accessible.Link
                    Accessible.name: qsTr("Open streaming settings")
                    TapHandler { onTapped: AppController.navigate("settings-streaming") }
                }
            }
            Repeater {
                model: [
                    {label: qsTr("Picture"), value: root.streamQualityLabel(), region: false},
                    {label: qsTr("Codec"), value: root.codecLabel(), region: false},
                    {label: qsTr("Region"), value: root.regionLabel(), region: true}
                ]
                Item {
                    required property var modelData
                    width: requestsColumn.width
                    height: 30
                    Accessible.role: Accessible.StaticText
                    Accessible.name: modelData.label + ": " + modelData.value
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.label
                        color: Theme.textMuted
                        font.family: Theme.bodyFont
                        font.pixelSize: 19
                        font.weight: Font.DemiBold
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8
                        Rectangle {
                            visible: modelData.region
                            anchors.verticalCenter: parent.verticalCenter
                            width: 8; height: 8; radius: 4; color: Theme.mint
                        }
                        Text {
                            text: modelData.value
                            color: Theme.label
                            font.family: Theme.bodyFont
                            font.pixelSize: 19
                            font.weight: Font.Black
                        }
                    }
                }
            }
            Text {
                width: parent.width
                text: qsTr("The rig decides the final stream. These are requests, shown before launch only.")
                wrapMode: Text.WordWrap
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 15
            }
        }
    }

    AppChrome {
        anchors.fill: parent
        title: ""
        currentRoute: "home"
        navVisible: false
        leftHints: [{glyph: "B", label: qsTr("Back")}]
        rightHints: [{glyph: "Y", label: favoriteButton.text}, {glyph: "A", label: play.text}]
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleChoiceSheet {
        objectName: "consoleLibraryOptionsSheet"
        anchors.fill: parent
        opened: root.libraryOptionsOpen
        eyebrow: qsTr("LIBRARY")
        title: qsTr("Library options")
        description: String(root.game.title || "")
        currentIndex: -1
        options: root.libraryOptions
        onChosen: index => root.runLibraryOption(root.libraryOptions[index].value)
        onDismissed: root.closeSheets()
    }

    ConsoleChoiceSheet {
        objectName: "consolePlatformSheet"
        anchors.fill: parent
        opened: root.platformSheetOpen
        eyebrow: qsTr("PLAY ON")
        title: qsTr("Choose a store")
        description: String(root.game.title || "")
        currentIndex: root.selectedVariantIndex
        options: root.variants.map(variant => ({label: ConsoleStores.label(variant.store), value: String(variant.id || ""),
            detail: ConsoleStores.ownershipLabel(variant)}))
        onChosen: index => {
            root.selectVariant(index)
            root.closeSheets()
        }
        onDismissed: root.closeSheets()
    }

    ConsoleWarningSheet {
        id: ownershipSheet
        objectName: "cloudOwnershipConfirmation"
        safeButtonObjectName: "cloudOwnershipCancel"
        actionButtonObjectName: "cloudOwnershipConfirm"
        readonly property var target: ShellStore.ownershipConfirmation
        readonly property bool removing: target !== null && target !== undefined && target.action === "remove"
        anchors.fill: parent
        opened: target !== null && target !== undefined
        eyebrow: qsTr("LIBRARY")
        title: removing ? qsTr("Remove this store version?") : qsTr("Do you already own this game?")
        message: !opened ? ""
            : removing
                ? qsTr("Remove the %1 version from your GeForce NOW library? This does not uninstall the game or revoke your license. A later store sync may restore it.").arg(ConsoleStores.label(target.store))
                : qsTr("This adds the %1 version to your GeForce NOW library. It does not buy the game or grant a license. You must already own it on %1.").arg(ConsoleStores.label(target.store))
        safeText: qsTr("Cancel")
        actionText: removing ? qsTr("Remove ownership") : qsTr("Confirm existing ownership")
        danger: removing
        onSafeRequested: ShellStore.ownershipConfirmation = null
        onActionRequested: ShellStore.confirmOwnership()
        onOpenedChanged: if (!opened) root.closeSheets()
    }

    Component.onCompleted: {
        if (initialPlatformOpen) {
            Qt.callLater(root.focusChips)
            if (!root.inlineVariants) {
                root.focusOrigin = moreStores
                root.platformSheetOpen = true
            }
        }
    }
}

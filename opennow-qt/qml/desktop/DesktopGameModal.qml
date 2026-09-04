import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property var game: ShellStore.selectedGame
    signal closeRequested()
    signal playRequested()
    focus: visible
    onVisibleChanged: if (visible) Qt.callLater(() => primaryAction.forceActiveFocus())

    readonly property int gutter: DesktopTokens.px(32)
    readonly property int dialogWidth: Math.min(DesktopTokens.px(896), Math.max(DesktopTokens.px(640), parent.width - gutter * 2))
    readonly property int dialogHeight: Math.min(DesktopTokens.px(666), Math.max(DesktopTokens.px(480), parent.height - gutter * 2))
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
        const parsed = Date.parse(raw)
        if (isNaN(parsed))
            return raw
        const deltaMs = Date.now() - parsed
        if (deltaMs < 0)
            return Qt.formatDate(new Date(parsed), Qt.DefaultLocaleShortDate)
        const minutes = Math.round(deltaMs / 60000)
        if (minutes < 1)
            return qsTr("Just now")
        if (minutes < 60)
            return qsTr("%1 minutes ago").arg(minutes)
        const hours = Math.round(minutes / 60)
        if (hours < 24)
            return qsTr("%1 hours ago").arg(hours)
        const days = Math.round(hours / 24)
        if (days < 14)
            return qsTr("%1 days ago").arg(days)
        return Qt.formatDate(new Date(parsed), Qt.DefaultLocaleShortDate)
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

    Rectangle {
        anchors.fill: parent
        color: "#C9000000"
        // This is a modal input shield as well as a scrim. MouseArea takes the
        // exclusive mouse grab so the release cannot activate a poster below
        // after closeRequested changes the route.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            hoverEnabled: true
            preventStealing: true
            onClicked: root.closeRequested()
            onWheel: wheel => wheel.accepted = true
        }
    }

    Rectangle {
        id: dialog
        width: root.dialogWidth
        height: root.dialogHeight
        anchors.centerIn: parent
        radius: DesktopTokens.px(18)
        color: "#ED0B101A"
        border.width: 1
        border.color: "#29FFFFFF"
        clip: true
        scale: root.visible ? 1 : 0.96
        opacity: root.visible ? 1 : 0
        Behavior on scale { NumberAnimation { duration: DesktopTokens.motionDuration; easing.type: Easing.OutBack } }
        Behavior on opacity { NumberAnimation { duration: DesktopTokens.quickDuration } }
        TapHandler { }

        RoundedArtwork {
            id: hero
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: Math.round(Math.min(DesktopTokens.px(250), dialog.height * 0.38))
            artwork: DesktopTokens.artworkUrl(root.game, true)
            cornerRadius: dialog.radius
            scrimStart: 0.08
            fallbackColor: "#1B2435"
        }
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: hero.bottom
            height: Math.round(hero.height * 0.52)
            gradient: Gradient {
                GradientStop { position: 0; color: "transparent" }
                GradientStop { position: 1; color: "#FA080D16" }
            }
        }
        Rectangle {
            anchors.left: parent.left
            anchors.leftMargin: DesktopTokens.px(21)
            anchors.top: parent.top
            anchors.topMargin: DesktopTokens.px(20)
            width: publisher.implicitWidth + DesktopTokens.px(22)
            height: DesktopTokens.px(26)
            radius: DesktopTokens.px(8)
            color: "#CC080B12"
            border.width: 1
            border.color: DesktopTokens.seam
            Text {
                id: publisher
                anchors.centerIn: parent
                text: root.game ? String(root.game.publisherName || qsTr("GEFORCE NOW")).toUpperCase() : qsTr("GEFORCE NOW")
                color: DesktopTokens.textBody
                font.family: DesktopTokens.monoFont
                font.pixelSize: DesktopTokens.px(9)
                font.weight: Font.DemiBold
                font.letterSpacing: 0.6
            }
        }
        DesktopButton {
            anchors.right: parent.right
            anchors.rightMargin: DesktopTokens.px(20)
            anchors.top: parent.top
            anchors.topMargin: DesktopTokens.px(20)
            width: DesktopTokens.px(24)
            height: DesktopTokens.px(24)
            glyph: "desktop-close.svg"
            glyphSize: DesktopTokens.px(14)
            leftPadding: 0
            rightPadding: 0
            onClicked: root.closeRequested()
        }
        Text {
            id: titleLabel
            anchors.left: parent.left
            anchors.leftMargin: DesktopTokens.px(21)
            anchors.right: parent.right
            anchors.rightMargin: DesktopTokens.px(21)
            anchors.bottom: badges.top
            anchors.bottomMargin: DesktopTokens.px(8)
            text: root.game ? String(root.game.title || qsTr("Game")) : qsTr("Game")
            color: DesktopTokens.text
            elide: Text.ElideRight
            font.family: DesktopTokens.displayFont
            font.pixelSize: DesktopTokens.px(30)
            font.weight: Font.Black
            font.letterSpacing: -0.8
        }
        Row {
            id: badges
            anchors.left: parent.left
            anchors.leftMargin: DesktopTokens.px(21)
            anchors.bottom: hero.bottom
            anchors.bottomMargin: DesktopTokens.px(16)
            spacing: DesktopTokens.px(8)
            Repeater {
                model: root.badgeLabels
                delegate: Rectangle {
                    required property string modelData
                    width: badgeText.implicitWidth + DesktopTokens.px(20)
                    height: DesktopTokens.px(24)
                    radius: DesktopTokens.px(8)
                    color: modelData === qsTr("READY TO PLAY") ? "#1F56E6A5" : "#0FFFFFFF"
                    border.width: 1
                    border.color: modelData === qsTr("READY TO PLAY") ? "#5256E6A5" : DesktopTokens.seam
                    Text {
                        id: badgeText
                        anchors.centerIn: parent
                        text: modelData
                        color: modelData === qsTr("READY TO PLAY") ? DesktopTokens.green : DesktopTokens.textBody
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: DesktopTokens.px(9)
                        font.weight: Font.DemiBold
                        font.letterSpacing: 0.5
                    }
                }
            }
        }

        Row {
            id: actions
            anchors.left: parent.left
            anchors.leftMargin: DesktopTokens.px(21)
            anchors.top: hero.bottom
            anchors.topMargin: DesktopTokens.px(18)
            spacing: DesktopTokens.px(10)
            DesktopButton {
                id: primaryAction
                width: DesktopTokens.px(156)
                height: DesktopTokens.px(40)
                primary: true
                glyph: "desktop-play.svg"
                glyphSize: DesktopTokens.px(12)
                text: qsTr("Start")
                shortcutText: qsTr("ENTER")
                onClicked: root.playRequested()
            }
            DesktopButton {
                width: DesktopTokens.px(110)
                height: DesktopTokens.px(40)
                glyph: "desktop-star.svg"
                glyphSize: DesktopTokens.px(15)
                text: qsTr("Favourite")
                onClicked: if (root.game) ShellStore.toggleFavorite(root.game)
            }
            DesktopButton {
                width: DesktopTokens.px(157)
                height: DesktopTokens.px(40)
                glyph: "desktop-folder.svg"
                glyphSize: DesktopTokens.px(15)
                text: root.game && ShellStore.isFavorite(root.game) ? qsTr("In favourites") : qsTr("Add to collection")
                onClicked: if (root.game) ShellStore.toggleFavorite(root.game)
            }
        }
        Rectangle {
            anchors.right: parent.right
            anchors.rightMargin: DesktopTokens.px(21)
            anchors.verticalCenter: actions.verticalCenter
            width: ownedLabel.implicitWidth + DesktopTokens.px(28)
            height: DesktopTokens.px(32)
            radius: DesktopTokens.px(9)
            color: "#CC070A11"
            border.width: 1
            border.color: DesktopTokens.seam
            Row {
                anchors.centerIn: parent
                spacing: DesktopTokens.px(8)
                Rectangle { width: 6; height: 6; radius: 3; color: root.isOwned ? DesktopTokens.green : DesktopTokens.textFaint }
                Text {
                    id: ownedLabel
                    text: root.ownershipText
                    color: DesktopTokens.textBody
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.px(9)
                    font.weight: Font.DemiBold
                    font.letterSpacing: 0.5
                }
            }
        }

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: actions.bottom
            anchors.topMargin: DesktopTokens.px(18)
            height: 1
            color: DesktopTokens.seamSoft
        }

        Item {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: actions.bottom
            anchors.bottom: footer.top
            anchors.leftMargin: DesktopTokens.px(21)
            anchors.rightMargin: DesktopTokens.px(21)
            anchors.topMargin: DesktopTokens.px(22)
            anchors.bottomMargin: DesktopTokens.px(12)

            Column {
                id: leftCol
                anchors.left: parent.left
                anchors.right: streamCard.left
                anchors.rightMargin: DesktopTokens.px(18)
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                spacing: DesktopTokens.px(14)
                Text {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    elide: Text.ElideRight
                    maximumLineCount: 3
                    text: root.game && root.game.description
                          ? String(root.game.description)
                          : qsTr("Stream this game instantly from your linked library. Your settings and session preferences are applied before the remote machine starts.")
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.px(12)
                    lineHeight: 1.45
                }
                Rectangle {
                    width: parent.width
                    height: DesktopTokens.px(62)
                    radius: DesktopTokens.px(12)
                    color: "#08FFFFFF"
                    border.width: 1
                    border.color: DesktopTokens.seamSoft
                    Row {
                        anchors.fill: parent
                        anchors.margins: DesktopTokens.px(14)
                        Repeater {
                            id: factRepeater
                            model: root.factItems
                            delegate: Item {
                                required property var modelData
                                width: parent.width / Math.max(1, factRepeater.count)
                                height: parent.height
                                Column {
                                    spacing: 5
                                    Text {
                                        text: modelData.l
                                        color: DesktopTokens.textFaint
                                        font.family: DesktopTokens.monoFont
                                        font.pixelSize: DesktopTokens.px(8)
                                        font.weight: Font.DemiBold
                                        font.letterSpacing: 0.6
                                    }
                                    Text {
                                        width: parent.parent.width - DesktopTokens.px(8)
                                        text: modelData.v
                                        color: modelData.l === qsTr("AVAILABLE") && root.gameAvailable ? DesktopTokens.green : DesktopTokens.textBody
                                        elide: Text.ElideRight
                                        font.family: DesktopTokens.bodyFont
                                        font.pixelSize: DesktopTokens.captionSize
                                        font.weight: Font.DemiBold
                                    }
                                }
                            }
                        }
                    }
                }
                Text {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: root.friendsNote
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.monoSize
                }
            }

            Rectangle {
                id: streamCard
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: Math.min(DesktopTokens.px(288), Math.max(DesktopTokens.px(220), parent.width * 0.34))
                radius: DesktopTokens.px(14)
                color: "#CC070A11"
                border.width: 1
                border.color: DesktopTokens.seamSoft
                Text {
                    x: DesktopTokens.px(16)
                    y: DesktopTokens.px(17)
                    text: qsTr("Stream for this game")
                    color: DesktopTokens.text
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.px(12)
                    font.weight: Font.Black
                }
                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: DesktopTokens.px(16)
                    y: DesktopTokens.px(19)
                    visible: root.membershipText !== ""
                    text: root.membershipText
                    color: DesktopTokens.amber
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.px(8)
                    font.weight: Font.DemiBold
                    font.letterSpacing: 0.6
                }
                Column {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: DesktopTokens.px(16)
                    anchors.rightMargin: DesktopTokens.px(16)
                    anchors.topMargin: DesktopTokens.px(51)
                    anchors.bottomMargin: DesktopTokens.px(16)
                    spacing: DesktopTokens.px(18)
                    Repeater {
                        model: root.streamRows
                        delegate: Item {
                            required property var modelData
                            width: parent.width
                            height: DesktopTokens.px(22)
                            Text {
                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.l
                                color: DesktopTokens.textMuted
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.captionSize
                            }
                            Text {
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.v
                                color: DesktopTokens.textHigh
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.monoSize
                                font.weight: Font.Bold
                            }
                        }
                    }
                    DesktopButton {
                        width: parent.width
                        height: DesktopTokens.controlHeight
                        text: qsTr("Edit stream settings")
                        onClicked: AppController.navigate("settings-streaming")
                    }
                }
            }
        }

        Rectangle {
            id: footer
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: DesktopTokens.px(44)
            color: "#E8070A11"
            Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
            Row {
                anchors.left: parent.left
                anchors.leftMargin: DesktopTokens.px(21)
                anchors.verticalCenter: parent.verticalCenter
                spacing: DesktopTokens.px(16)
                DesktopKeyHint { keyText: "Esc"; label: qsTr("Close") }
                DesktopKeyHint { keyText: qsTr("Enter"); label: qsTr("Start") }
                DesktopKeyHint { keyText: "F"; label: qsTr("Favourite") }
            }
            Text {
                anchors.right: parent.right
                anchors.rightMargin: DesktopTokens.px(21)
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("SESSION STARTS IN %1").arg(root.regionText)
                color: DesktopTokens.textFaint
                font.family: DesktopTokens.monoFont
                font.pixelSize: DesktopTokens.px(9)
                font.weight: Font.DemiBold
                font.letterSpacing: 0.6
            }
        }
    }
    Keys.onEscapePressed: root.closeRequested()
    Keys.onReturnPressed: root.playRequested()
}

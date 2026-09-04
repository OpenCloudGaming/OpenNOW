import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

FocusScope {
    id: root
    anchors.fill: parent
    clip: true

    property int selectedSection: 0
    property string dropdownKey: ""
    signal requestConsoleMode(bool enabled)

    readonly property var sections: [
        { kind: "header", label: "ACCOUNT" },
        { kind: "item", label: "Profile", page: 0 },
        { kind: "item", label: "Subscription", badge: root.liveTierBadge(), page: 1 },
        { kind: "item", label: "Linked stores", page: 2 },
        { kind: "spacer" },
        { kind: "header", label: "STREAMING" },
        { kind: "item", label: "Stream", page: 3 },
        { kind: "item", label: "Audio", page: 4 },
        { kind: "item", label: "Controllers", page: 5 },
        { kind: "item", label: "Network", page: 6 },
        { kind: "spacer" },
        { kind: "header", label: "APP" },
        { kind: "item", label: "Interface", page: 7 },
        { kind: "item", label: "Themes", badge: String(ShellStore.settings.themePack || "nocturne").toUpperCase(), page: 8 },
        { kind: "item", label: "Console mode", page: 9 },
        { kind: "item", label: "Shortcuts", page: 10 },
        { kind: "item", label: "About", page: 11 },
        { kind: "version", label: String(ShellStore.updaterState.currentVersion || Qt.application.version || qsTr("unknown")) }
    ]
    readonly property var pageTitles: ["Profile", "Subscription", "Linked stores", "Stream", "Audio", "Controllers", "Network", "Interface", "Themes", "Console mode", "Shortcuts", "About"]
    readonly property var pageSubtitles: [
        "Your NVIDIA identity and the profiles saved on this device.",
        "Entitlements reported by your GeForce NOW account.",
        "Link a store once and its cloud-ready games show up in your library.",
        "Defaults for every session. Resolution and frame rate follow your membership entitlements.",
        "Output and input behavior for every session.",
        "Pads, glyphs and what happens when one wakes up.",
        "Region, bitrate and proxy settings used by OpenNOW.",
        "How the desktop shell looks, starts and notifies you.",
        "Built-in palettes for the OpenNOW shell.",
        "The gamepad-first shell inside the same app — for the TV or the Deck.",
        "Current desktop, console and in-stream bindings.",
        "Build, updates and the fine print."
    ]
    readonly property var pageComponents: [profilePage, subscriptionPage, storesPage, streamPage, audioPage, controllersPage, networkPage, interfacePage, themesPage, consolePage, shortcutsPage, aboutPage]

    Component.onCompleted: {
        ShellStore.refreshRegions()
        ShellStore.refreshGameAccounts()
    }

    function boolSetting(key, fallbackValue) {
        return ShellStore.settings[key] === undefined ? fallbackValue : Boolean(ShellStore.settings[key])
    }

    function valueSetting(key, fallbackValue) {
        const value = ShellStore.settings[key]
        return value === undefined || value === null || value === "" ? fallbackValue : value
    }

    function setSetting(key, value) {
        ShellStore.applySetting(key, value)
        ShellStore.setSetting(key, value)
        if (key === "resolution")
            Qt.callLater(root.clampFpsToEntitlement)
    }

    function setChoice(key, value) {
        const normalized = String(value || "").toLowerCase()
        const codec = String(root.valueSetting("codec", "auto")).toLowerCase()
        if (key === "colorQuality") {
            if (codec === "h264" && normalized !== "8bit_420")
                return
            if (codec === "av1" && normalized.indexOf("444") >= 0)
                return
        }
        const currentQuality = String(root.valueSetting("colorQuality", "8bit_420"))
        root.setSetting(key, value)
        if (key === "codec") {
            if (normalized === "h264" && currentQuality !== "8bit_420")
                root.setSetting("colorQuality", "8bit_420")
            else if (normalized === "av1" && currentQuality.indexOf("444") >= 0)
                root.setSetting("colorQuality", currentQuality.replace("444", "420"))
        }
    }

    function choices(values) {
        return values.map(value => typeof value === "object" ? value : ({ kind: "choice", label: String(value), value: value }))
    }

    function colorQualityItems() {
        const codec = String(root.valueSetting("codec", "auto")).toLowerCase()
        const h264 = codec === "h264"
        const chroma444Unavailable = h264 || codec === "av1"
        return [
            {kind:"choice", label:qsTr("8-bit, YUV 4:2:0"), detail:qsTr("All codecs"), value:"8bit_420"},
            {kind:"choice", label:qsTr("8-bit, YUV 4:4:4"), detail:chroma444Unavailable ? qsTr("H.265 required") : qsTr("Sharper color"), value:"8bit_444", disabled:chroma444Unavailable},
            {kind:"choice", label:qsTr("10-bit, YUV 4:2:0"), detail:h264 ? qsTr("H.265 / AV1 required") : qsTr("Smoother gradients"), value:"10bit_420", disabled:h264},
            {kind:"choice", label:qsTr("10-bit, YUV 4:4:4"), detail:chroma444Unavailable ? qsTr("H.265 required") : qsTr("Highest color quality"), value:"10bit_444", disabled:chroma444Unavailable}
        ]
    }

    function colorQualityFooter() {
        const codec = String(root.valueSetting("codec", "auto")).toLowerCase()
        if (codec === "h264")
            return qsTr("H.264 supports 8-bit YUV 4:2:0 only")
        if (codec === "av1")
            return qsTr("AV1 supports YUV 4:2:0; use H.265 for 4:4:4")
        return qsTr("4:4:4 profiles use H.265")
    }

    function colorQualityLabel() {
        const labels = {
            "8bit_420": qsTr("8-bit, YUV 4:2:0"),
            "8bit_444": qsTr("8-bit, YUV 4:4:4"),
            "10bit_420": qsTr("10-bit, YUV 4:2:0"),
            "10bit_444": qsTr("10-bit, YUV 4:4:4")
        }
        const value = String(root.valueSetting("colorQuality", "8bit_420"))
        return labels[value] || labels["8bit_420"]
    }

    function openChoices(anchor, key, items, footer) {
        dropdownKey = key
        choiceOverlay.showFor(anchor, items, ShellStore.settings[key], footer || "")
    }

    function liveTierBadge() {
        const tier = ShellStore.subscription ? String(ShellStore.subscription.membershipTier || "") : ""
        return tier === "" ? "" : tier.toUpperCase()
    }

    function profileUser() {
        return ShellStore.authSession && ShellStore.authSession.user ? ShellStore.authSession.user : ({})
    }

    function profileName() {
        return String(root.profileUser().displayName || qsTr("Guest"))
    }

    function profileInitial() {
        const name = root.profileName()
        return name.length ? name.charAt(0).toUpperCase() : "?"
    }

    function profileEmail() {
        return String(root.profileUser().email || "")
    }

    function profileSubtitle() {
        if (!ShellStore.signedIn)
            return qsTr("Not signed in")
        const email = root.profileEmail()
        if (email !== "")
            return qsTr("%1 · NVIDIA account linked").arg(email)
        return qsTr("NVIDIA account linked")
    }

    function maskedEmail() {
        const email = root.profileEmail()
        const at = email.indexOf("@")
        if (at <= 0)
            return ShellStore.signedIn ? qsTr("NVIDIA account") : qsTr("Not signed in")
        return email.charAt(0) + "•••••" + email.slice(at)
    }

    function regionGroup(name) {
        const n = String(name || "").toLowerCase()
        if (n.indexOf("us ") === 0 || n.indexOf("us-") === 0 || n.indexOf("ca ") === 0 || n.indexOf("canada") >= 0 || n.indexOf("north america") >= 0)
            return qsTr("NORTH AMERICA")
        if (n.indexOf("eu ") === 0 || n.indexOf("eu-") === 0 || n.indexOf("uk") === 0 || n.indexOf("europe") >= 0 || n.indexOf("london") >= 0 || n.indexOf("frankfurt") >= 0 || n.indexOf("amsterdam") >= 0)
            return qsTr("EUROPE")
        if (n.indexOf("jp") === 0 || n.indexOf("kr") === 0 || n.indexOf("sg") === 0 || n.indexOf("au") === 0 || n.indexOf("asia") >= 0 || n.indexOf("tokyo") >= 0 || n.indexOf("seoul") >= 0 || n.indexOf("singapore") >= 0 || n.indexOf("sydney") >= 0 || n.indexOf("india") >= 0)
            return qsTr("ASIA PACIFIC")
        if (n.indexOf("br") === 0 || n.indexOf("south america") >= 0 || n.indexOf("brazil") >= 0 || n.indexOf("sao") >= 0)
            return qsTr("SOUTH AMERICA")
        if (n.indexOf("me ") === 0 || n.indexOf("middle") >= 0 || n.indexOf("uae") >= 0)
            return qsTr("MIDDLE EAST")
        return qsTr("OTHER")
    }

    function regionChoiceItems() {
        const items = [{ kind: "choice", label: qsTr("Automatic"), detail: qsTr("Lowest latency"), value: "" }]
        const regions = (ShellStore.regions || []).slice()
        let lastGroup = ""
        for (let i = 0; i < regions.length; ++i) {
            const region = regions[i]
            const group = root.regionGroup(region.name)
            if (group !== lastGroup) {
                items.push({ kind: "heading", label: group })
                lastGroup = group
            }
            const ping = root.regionPingMs(region.url)
            items.push({
                kind: "choice",
                label: region.name,
                detail: ping === null ? "" : (ping + " ms"),
                detailColor: ping === null ? "" : root.pingRankColor(ping),
                value: region.url
            })
        }
        return items
    }

    function currentRegionLabel() {
        const selected = String(valueSetting("region", ""))
        if (selected === "")
            return qsTr("Automatic")
        const regions = ShellStore.regions || []
        for (let i = 0; i < regions.length; ++i) {
            if (regions[i].name === selected || regions[i].url === selected) {
                const ping = ShellStore.regionPingResults ? ShellStore.regionPingResults[regions[i].url] : undefined
                const name = regions[i].name
                return ping === undefined || ping === null || ping === "" ? name : (name + " · " + ping + " ms")
            }
        }
        return selected
    }

    function regionPingMs(url) {
        const raw = ShellStore.regionPingResults ? ShellStore.regionPingResults[url] : undefined
        const n = Number(raw)
        return Number.isFinite(n) ? n : null
    }

    function currentRegionPing() {
        const selected = String(root.valueSetting("region", ""))
        if (selected === "")
            return null
        const regions = ShellStore.regions || []
        for (let i = 0; i < regions.length; ++i) {
            if (regions[i].name === selected || regions[i].url === selected)
                return root.regionPingMs(regions[i].url)
        }
        return null
    }

    // Ping quality ramp mirrors Electron's region picker: green <30,
    // lime <80, amber <150, red beyond. Unmeasured regions stay muted.
    function pingRankColor(ms) {
        if (ms === null || ms === undefined)
            return Theme.textMuted
        if (ms < 30)
            return "#58d98a"
        if (ms < 80)
            return "#84cc16"
        if (ms < 150)
            return "#eab308"
        return "#ef4444"
    }

    function pingBarsLit(ms) {
        if (ms === null || ms === undefined)
            return 0
        if (ms < 80)
            return 3
        if (ms < 150)
            return 2
        return 1
    }

    function themeMeta(id) {
        const packs = {
            aurora: { name: qsTr("Aurora"), blurb: qsTr("Cool teal shell with a mint focus ring."), accent: "#56E6A5" },
            nocturne: { name: qsTr("Nocturne"), blurb: qsTr("Near-black nocturne shell with a sky focus ring."), accent: "#7FD4FF" },
            kraft: { name: qsTr("Kraft"), blurb: qsTr("Warm brown shell with a brass focus ring."), accent: "#C6A46A" },
            phosphor: { name: qsTr("Mint"), blurb: qsTr("Deep green shell with a phosphor focus ring."), accent: "#56E6A5" },
            hibiscus: { name: qsTr("Sunset"), blurb: qsTr("Wine shell with a rose focus ring."), accent: "#FF8A80" },
            chapel: { name: qsTr("Chapel"), blurb: qsTr("Violet night shell with a gold focus ring."), accent: "#FFD166" },
            bone: { name: qsTr("Bone"), blurb: qsTr("Light paper shell for daytime use."), accent: "#C6A46A" },
            cobalt: { name: qsTr("Cobalt"), blurb: qsTr("Light blue-white shell with a cobalt focus ring."), accent: "#7FD4FF" }
        }
        return packs[id] || { name: String(id || qsTr("Theme")), blurb: qsTr("Installed theme pack.") }
    }

    function planChips() {
        const sub = ShellStore.subscription
        if (!sub)
            return [ShellStore.signedIn ? qsTr("Entitlements unavailable") : qsTr("Sign in to load entitlements")]
        const chips = []
        const resolutions = sub.entitledResolutions || []
        for (let i = 0; i < Math.min(4, resolutions.length); ++i) {
            const item = resolutions[i]
            const width = Number(item.width || 0)
            const height = Number(item.height || 0)
            const fps = Number(item.fps || 0)
            if (width >= 3840)
                chips.push("4K" + (fps ? " · " + fps + " FPS" : ""))
            else if (width >= 2560)
                chips.push("1440P" + (fps ? " · " + fps + " FPS" : ""))
            else if (width >= 1920)
                chips.push("1080P" + (fps ? " · " + fps + " FPS" : ""))
            else if (height > 0)
                chips.push(height + "P" + (fps ? " · " + fps + " FPS" : ""))
        }
        if (sub.isUnlimited)
            chips.push(qsTr("UNLIMITED"))
        else if (sub.remainingHours !== undefined && sub.remainingHours !== null)
            chips.push(qsTr("%1 h left").arg(Math.max(0, Math.round(Number(sub.remainingHours)))))
        if (chips.length === 0)
            chips.push(String(sub.membershipTier || qsTr("Unavailable")).toUpperCase())
        return chips
    }

    // Frame-rate helpers share ShellStore's entitlement logic (mirroring
    // Electron's getFpsForResolution); these are display wrappers only.
    function fpsEntitlementKnown() {
        return Boolean(ShellStore.subscription)
    }

    function currentResolutionValue() {
        return String(root.valueSetting("resolution", "1920x1080"))
    }

    function unentitledFpsValues() {
        return ShellStore.unentitledFpsValues(root.currentResolutionValue())
    }

    function fpsEntitlementNote() {
        if (!root.fpsEntitlementKnown())
            return ShellStore.signedIn
                ? qsTr("Loading your membership entitlements…")
                : qsTr("Sign in to unlock rates beyond 60 FPS")
        const entitled = ShellStore.entitledFpsForResolution(root.currentResolutionValue())
        const tier = root.liveTierBadge() || qsTr("Membership")
        if (entitled.length === 0)
            return qsTr("%1 · no exact entitlement for this resolution").arg(tier)
        const max = entitled[entitled.length - 1]
        return qsTr("%1 · up to %2 FPS at %3").arg(tier).arg(max)
            .arg(root.currentResolutionValue().replace("x", "×"))
    }

    function fpsLockedHint() {
        const tier = root.liveTierBadge()
        return tier
            ? qsTr("Not entitled on %1 — upgrade on NVIDIA to unlock").arg(tier)
            : qsTr("Not entitled on your current membership")
    }

    function clampFpsToEntitlement() {
        ShellStore.clampFpsToEntitlement()
    }

    function storeLetter(account) {
        const provider = String(account.provider || account.label || "").trim()
        return provider ? provider.charAt(0).toUpperCase() : ""
    }

    function storeIcon(account) {
        const provider = String(account.provider || account.label || "").toLowerCase()
        const base = "qrc:/qt/qml/OpenNOW/res/icons/"
        if (provider.indexOf("steam") >= 0) return base + "store-steam.svg"
        if (provider.indexOf("epic") >= 0) return base + "store-epic.svg"
        if (provider.indexOf("ubisoft") >= 0 || provider.indexOf("uplay") >= 0) return base + "store-ubisoft.svg"
        if (provider.indexOf("battle") >= 0) return base + "store-battlenet.svg"
        return base + "desktop-nav-store.svg"
    }

    function storeAccent(account) {
        const provider = String(account.provider || account.label || "").toLowerCase()
        if (provider.indexOf("steam") >= 0) return Theme.cartSteam
        if (provider.indexOf("epic") >= 0) return Theme.cartEpic
        if (provider.indexOf("ubisoft") >= 0 || provider.indexOf("uplay") >= 0) return Theme.cartUbisoft
        if (provider.indexOf("xbox") >= 0) return Theme.cartXbox
        if (provider.indexOf("gog") >= 0) return Theme.cartGog
        if (provider.indexOf("battle") >= 0) return Theme.cartBattlenet
        return Qt.rgba(1, 1, 1, 0.08)
    }

    function storeStatus(account) {
        if (account.status === "expired")
            return { text: qsTr("EXPIRED"), color: Theme.yellow, action: qsTr("Reconnect"), connected: false, primary: true }
        if (account.status === "sync_error")
            return { text: qsTr("SYNC ISSUE"), color: Theme.coral, action: qsTr("Resync"), connected: true }
        if (account.isConnected || account.status === "connected")
            return { text: qsTr("LINKED"), color: DesktopTokens.green, action: account.supportsSync ? qsTr("Resync") : qsTr("Unlink"), connected: true }
        return { text: qsTr("NOT LINKED"), color: Theme.textMuted, action: qsTr("Link"), connected: false }
    }

    function storeDescription(account) {
        if (account.displayName)
            return account.displayName
        if (account.isConnected && account.syncedGames !== undefined && account.syncedGames !== null)
            return qsTr("%1 cloud-ready games synced").arg(account.syncedGames)
        if (account.isConnected)
            return qsTr("Connected through your NVIDIA account")
        return qsTr("Link this store on NVIDIA to add its games to your library")
    }

    function runStoreAction(account) {
        const status = storeStatus(account)
        if (status.action === qsTr("Resync"))
            ShellStore.syncGameAccount(account.provider)
        else if (status.connected)
            ShellStore.unlinkGameAccount(account.provider)
        else
            ShellStore.startAccountLink(account.provider)
    }

    function projectLinks() {
        return [
            {id: "source", label: qsTr("Source on GitHub"), hint: "↗"},
            {id: "issues", label: qsTr("Report an issue"), hint: "↗"},
            {id: "diagnostics", label: qsTr("Copy diagnostics"), hint: "NO PERSONAL DATA"},
            {id: "captures", label: qsTr("Reveal captures folder"), hint: ShellStore.mediaRootPath ? "↗" : "…"}
        ]
    }

    function runProjectLink(link) {
        if (!link)
            return
        if (link.id === "source")
            AppController.openExternalUrl("https://github.com/OpenCloudGaming/OpenNOW")
        else if (link.id === "issues")
            AppController.openExternalUrl("https://github.com/OpenCloudGaming/OpenNOW/issues")
        else if (link.id === "diagnostics")
            ShellStore.exportDiagnostics()
        else if (link.id === "captures") {
            if (ShellStore.mediaRootPath)
                AppController.openLocalPath(ShellStore.mediaRootPath, false)
            else
                ShellStore.refreshMedia()
        }
    }

    function resolutionItems() {
        return [
            {kind:"heading", label:"16:9 STANDARD"},
            {kind:"choice", label:"720p", detail:"1280×720", value:"1280x720"},
            {kind:"choice", label:"1080p", detail:"1920×1080", value:"1920x1080"},
            {kind:"choice", label:"1440p", detail:"2560×1440", value:"2560x1440"},
            {kind:"choice", label:"4K", detail:"3840×2160", value:"3840x2160"},
            {kind:"heading", label:"16:10 WIDESCREEN"},
            {kind:"choice", label:"720p · WXGA · WSXGA", detail:"1280×800 → 1680×1050", value:"1680x1050"},
            {kind:"choice", label:"1200p · 1600p · 4K", detail:"1920×1200 → 3840×2400", value:"2560x1600"},
            {kind:"heading", label:"21:9 ULTRAWIDE"},
            {kind:"choice", label:"UW 1080p · UW 1440p", detail:"2560×1080 · 3440×1440", value:"3440x1440"},
            {kind:"heading", label:"32:9 SUPER ULTRAWIDE"},
            {kind:"choice", label:"Super ultrawide", detail:"5120×1440", value:"5120x1440"}
        ]
    }

    Rectangle {
        id: settingsRail
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.leftMargin: DesktopTokens.px(24)
        anchors.topMargin: DesktopTokens.px(18)
        anchors.bottomMargin: DesktopTokens.px(18)
        width: DesktopTokens.px(268)
        color: "transparent"

        Column {
            width: parent.width
            Repeater {
                model: root.sections
                delegate: Item {
                    required property var modelData
                    width: settingsRail.width
                    height: modelData.kind === "header" ? DesktopTokens.px(36) : modelData.kind === "spacer" ? DesktopTokens.px(16) : modelData.kind === "version" ? DesktopTokens.px(40) : DesktopTokens.px(48)

                    Text {
                        visible: modelData.kind === "header"
                        anchors.left: parent.left
                        anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.label || ""
                        color: Qt.rgba(1,1,1,0.32)
                        font.family: Theme.monoFont
                        font.pixelSize: DesktopTokens.microSize
                        font.weight: Font.Bold
                        font.letterSpacing: 1.1
                    }

                    Button {
                        id: railButton
                        visible: modelData.kind === "item"
                        anchors.fill: parent
                        padding: 0
                        hoverEnabled: true
                        onClicked: root.selectedSection = modelData.page
                        background: Rectangle {
                            radius: 10
                            color: modelData.page === root.selectedSection ? Qt.rgba(1,1,1,0.10)
                                 : railButton.hovered ? Qt.rgba(1,1,1,0.045) : "transparent"
                            Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
                        }
                        contentItem: Item {
                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: 12
                                anchors.right: parent.right
                                anchors.rightMargin: modelData.badge ? 86 : 30
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.label || ""
                                color: modelData.page === root.selectedSection ? Theme.label : Qt.rgba(1,1,1,0.64)
                                font.family: Theme.bodyFont
                                font.pixelSize: DesktopTokens.headingSize
                                font.weight: modelData.page === root.selectedSection ? Font.Bold : Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Text {
                                visible: Boolean(modelData.badge)
                                anchors.right: parent.right
                                anchors.rightMargin: modelData.page === root.selectedSection ? 24 : 12
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.badge || ""
                                color: modelData.label === "Subscription" ? Theme.violet : Qt.rgba(1,1,1,0.34)
                                font.family: Theme.monoFont
                                font.pixelSize: DesktopTokens.microSize
                                font.weight: Font.Bold
                            }
                            Rectangle {
                                visible: modelData.page === root.selectedSection
                                width: 4
                                height: 18
                                radius: 2
                                color: DesktopTokens.focus
                                anchors.right: parent.right
                                anchors.rightMargin: 9
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                    }

                    Text {
                        visible: modelData.kind === "version"
                        anchors.left: parent.left
                        anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.label || ""
                        color: Qt.rgba(1,1,1,0.26)
                        font.family: Theme.monoFont
                        font.pixelSize: DesktopTokens.tinySize
                    }
                }
            }
        }
    }

    Item {
        id: contentLane
        anchors.left: settingsRail.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.leftMargin: 20
        anchors.rightMargin: 24
        anchors.topMargin: 18
        anchors.bottomMargin: 18

        Text {
            id: pageTitle
            text: root.pageTitles[root.selectedSection]
            color: Theme.label
            font.family: Theme.displayFont
            font.pixelSize: DesktopTokens.titleSize
            font.weight: Font.Black
        }
        Text {
            anchors.left: parent.left
            anchors.top: pageTitle.bottom
            anchors.topMargin: 3
            text: root.pageSubtitles[root.selectedSection]
            color: Qt.rgba(1,1,1,0.50)
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            font.weight: Font.Medium
        }

        Flickable {
            id: contentFlick
            x: 0
            y: 72
            width: parent.width
            height: parent.height - 72
            contentWidth: width
            contentHeight: pageLoader.childrenRect.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            Loader {
                id: pageLoader
                width: contentFlick.width
                sourceComponent: root.pageComponents[root.selectedSection]
                onLoaded: contentFlick.contentY = 0
            }
        }
    }

    DesktopSettingsDropdown {
        id: choiceOverlay
        onChosen: value => root.setChoice(root.dropdownKey, value)
    }

    Connections {
        target: ShellStore
        function onSubscriptionChanged() { root.clampFpsToEntitlement() }
    }

    Component {
        id: profilePage
        Column {
            width: contentFlick.width
            spacing: 14
            DesktopSettingsPanel {
                width: parent.width
                padding: 18
                Item {
                    width: parent.width
                    height: 64
                    Rectangle {
                        width: 56; height: 56; radius: 28
                        color: Qt.rgba(1,1,1,0.16)
                        border.width: 1; border.color: Qt.rgba(1,1,1,0.24)
                        Text { anchors.centerIn: parent; text: root.profileInitial(); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(22); font.weight: Font.Black }
                    }
                    Column {
                        x: 82; anchors.verticalCenter: parent.verticalCenter; spacing: 3
                        Row {
                            spacing: 10
                            Text { text: root.profileName(); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black }
                            Rectangle {
                                width: tier.implicitWidth + 12; height: 20; radius: 5; color: Qt.rgba(0.55,0.40,1,0.20)
                                Text { id: tier; anchors.centerIn: parent; text: root.liveTierBadge() || "—"; color: "#C7B5FF"; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold }
                            }
                        }
                        Text { text: root.profileSubtitle(); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                    }
                    DesktopSettingsButton {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: ShellStore.signedIn ? qsTr("Manage") : qsTr("Sign in")
                        compact: true
                        onClicked: AppController.navigate(ShellStore.signedIn ? "accounts" : "sign-in")
                    }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; title: qsTr("NVIDIA account"); description: qsTr("Bring your own account · OpenNOW never stores your password"); value: root.maskedEmail(); DesktopSettingsButton { text: qsTr("Manage"); compact: true; onClicked: AppController.navigate(ShellStore.signedIn ? "accounts" : "sign-in") } }
                DesktopSettingsRow { width: parent.width; title: qsTr("This device"); description: qsTr("Profiles and settings are stored locally"); value: qsTr("Current device") }
                DesktopSettingsRow { width: parent.width; title: qsTr("Saved profiles"); description: qsTr("Profiles stored in the OS credential store on this PC"); value: qsTr("%1 saved").arg(ShellStore.savedAccounts.length); showDivider: false; DesktopSettingsButton { text: qsTr("Review"); compact: true; onClicked: AppController.navigate("accounts") } }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 58; title: "Show what I am playing"; description: "Friends see the game and how long you have been in it"; DesktopSettingsToggle { checked: root.boolSetting("discordRichPresence", false); onValueChangedByUser: value => root.setSetting("discordRichPresence", value) } }
                DesktopSettingsRow { width: parent.width; rowHeight: 58; title: qsTr("GeForce NOW invitations"); description: qsTr("NVIDIA does not expose invitation services to third-party clients"); value: qsTr("UNAVAILABLE") }
                DesktopSettingsRow { width: parent.width; rowHeight: 58; title: "Crash reports"; description: "Off by default · OpenNOW ships no analytics or trackers"; showDivider: false; DesktopSettingsToggle { checked: ShellStore.settings.errorReportingConsent === "granted"; onValueChangedByUser: value => root.setSetting("errorReportingConsent", value ? "granted" : "denied") } }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Item {
                    width: parent.width; height: 36
                    Column { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                        Text { text: "Local data"; color: Qt.rgba(1,1,1,0.88); font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold }
                        Text { text: "Auth tokens, library cache and themes live in ~/.opennow · nothing leaves this device"; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                    }
                    Row { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 10
                        DesktopSettingsButton { text: "Sign out"; danger: true; onClicked: ShellStore.logout() }
                    }
                }
            }
        }
    }

    Component {
        id: subscriptionPage
        Column {
            width: contentFlick.width; spacing: 14
            DesktopSettingsPanel {
                width: parent.width; padding: 20
                Item {
                    width: parent.width; height: 86
                    Column { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; spacing: 7
                        Text { text: "CURRENT PLAN"; color: "#BBA2FF"; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold; font.letterSpacing: 1.1 }
                        Text { text: root.liveTierBadge() || "—"; color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(28); font.weight: Font.Black }
                        Row { spacing: 7
                            Repeater { model: root.planChips()
                                delegate: Rectangle { required property var modelData; width: chip.implicitWidth + 14; height: 22; radius: 6; color: Qt.rgba(1,1,1,0.07); border.width: 1; border.color: Qt.rgba(1,1,1,0.10); Text { id: chip; anchors.centerIn: parent; text: modelData; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold } }
                            }
                        }
                    }
                    Column { anchors.right: parent.right; spacing: 2
                        Text { anchors.right: parent.right; text: qsTr("MANAGED ON NVIDIA"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold }
                        Text { anchors.right: parent.right; text: qsTr("Not sold by OpenNOW"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Black }
                        Row { spacing: 8; topPadding: 8
                            DesktopSettingsButton { text: qsTr("Manage on NVIDIA"); primary: true; onClicked: Qt.openUrlExternally("https://www.nvidia.com/en-us/account/") }
                        }
                    }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Item { width: parent.width; height: 50
                    Column { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                        Text { text: qsTr("Entitlements reported by NVIDIA"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold }
                        Text { text: root.planChips().join(" · "); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.microSize; font.weight: Font.Bold }
                    }
                    Text { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: ShellStore.subscription ? qsTr("LIVE ACCOUNT DATA") : qsTr("UNAVAILABLE"); color: ShellStore.subscription ? DesktopTokens.green : Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Item { width: parent.width; height: 40
                    Text { anchors.left: parent.left; anchors.right: refresh.left; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; wrapMode: Text.WordWrap; text: qsTr("OpenNOW is a client, not a reseller. Plans, billing and rig availability are handled by NVIDIA — we only read your entitlements to decide which stream options to offer."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                    DesktopSettingsButton { id: refresh; anchors.right: parent.right; text: "Refresh entitlements"; compact: true; onClicked: ShellStore.refreshAccountServices() }
                }
            }
        }
    }

    Component {
        id: storesPage
        Column {
            width: contentFlick.width; spacing: 14
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Item { width: parent.width; height: 38
                    Column { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                        Row { spacing: 8
                            Text { text: "Library sync"; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold }
                            Text { text: ShellStore.gameAccountsState === "ready" ? qsTr("LIVE") : ShellStore.gameAccountsState === "error" ? qsTr("UNAVAILABLE") : qsTr("LOADING"); color: ShellStore.gameAccountsState === "ready" ? DesktopTokens.green : Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold }
                        }
                        Text { text: qsTr("%1 stores from your NVIDIA account").arg((ShellStore.gameAccounts || []).length); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                    }
                    Row { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 14
                        DesktopSettingsButton { text: qsTr("Sync now"); onClicked: ShellStore.refreshGameAccounts() }
                    }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Repeater {
                    model: ShellStore.gameAccounts
                    delegate: DesktopSettingsRow {
                        required property int index
                        required property var modelData
                        readonly property var status: root.storeStatus(modelData)
                        width: parent.width
                        rowHeight: 62
                        leadingLetter: root.storeLetter(modelData)
                        leadingIcon: root.storeIcon(modelData)
                        leadingColor: root.storeAccent(modelData)
                        title: modelData.label || modelData.provider
                        description: root.storeDescription(modelData)
                        showDivider: index < (ShellStore.gameAccounts || []).length - 1
                        Text {
                            width: 100
                            height: 30
                            text: status.text
                            color: status.color
                            font.family: Theme.monoFont
                            font.pixelSize: DesktopTokens.tinySize
                            font.weight: Font.Bold
                            horizontalAlignment: Text.AlignRight
                            verticalAlignment: Text.AlignVCenter
                        }
                        DesktopSettingsButton {
                            text: status.action
                            compact: true
                            primary: Boolean(status.primary)
                            onClicked: root.runStoreAction(modelData)
                        }
                    }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Text { width: parent.width; wrapMode: Text.WordWrap; text: qsTr("Linking happens on NVIDIA's side. OpenNOW opens the store login in your browser and only keeps the resulting session token, encrypted, on this device."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
            }
        }
    }

    Component {
        id: streamPage
        Column {
            width: contentFlick.width; spacing: 14
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 44; title: qsTr("Display"); description: qsTr("The Qt stream surface uses the current display"); value: qsTr("CURRENT DISPLAY") }
                DesktopSettingsRow { width: parent.width; rowHeight: 64; title: "Resolution"; description: "Exact stream size, grouped by aspect ratio"
                    DesktopSettingsButton { id: resolutionButton; menu: true; text: String(root.valueSetting("resolution", "1920x1080")).replace("x", "×"); compact: true; onClicked: root.openChoices(resolutionButton, "resolution", root.resolutionItems()) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 72; title: "Frame rate"; description: root.fpsEntitlementNote()
                    DesktopSettingsSegmented { options: ["AUTO","30","60","90","120","144","165","240"]; optionWidth: 58; selectedIndex: Math.max(0, ["AUTO","30","60","90","120","144","165","240"].indexOf(Number(root.valueSetting("fps", 60)) === 0 ? "AUTO" : String(Number(root.valueSetting("fps", 60))))); disabledValues: root.unentitledFpsValues(); disabledHint: root.fpsLockedHint(); onSelected: (index, value) => root.setSetting("fps", value === "AUTO" ? 0 : Number(value)) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 64; title: qsTr("Color quality"); description: qsTr("Bit depth and chroma format for new sessions"); showDivider: false
                    DesktopSettingsButton { id: colorQualityButton; menu: true; text: root.colorQualityLabel(); compact: true; onClicked: root.openChoices(colorQualityButton, "colorQuality", root.colorQualityItems(), root.colorQualityFooter()) }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 64; title: qsTr("Bitrate ceiling"); description: qsTr("Maximum requested stream bitrate"); showDivider: true
                    DesktopSettingsSlider { from: 10; to: 200; stepSize: 5; value: Number(root.valueSetting("maxBitrateMbps", 75)); suffix: " Mbps"; onCommitted: value => root.setSetting("maxBitrateMbps", Math.round(value)) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 64; title: "Codec"; description: "Automatic chooses from codecs reported by the native streamer"
                    DesktopSettingsButton { id: codecButton; menu: true; text: String(root.valueSetting("codec", "auto")).toUpperCase() + (String(root.valueSetting("codec", "auto")).toLowerCase() === "av1" ? " · RECOMMENDED" : ""); compact: true; onClicked: root.openChoices(codecButton, "codec", root.choices([{kind:"choice",label:"Automatic",detail:"Best available",value:"auto"},{kind:"choice",label:"AV1",detail:"Recommended",value:"av1"},{kind:"choice",label:"H.265",detail:"Efficient",value:"h265"},{kind:"choice",label:"H.264",detail:"Compatible",value:"h264"}])) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 56; title: "Reflex low latency"; description: "Requested per session when the game supports it"
                    DesktopSettingsToggle { checked: root.boolSetting("enableCloudGsync", true); onValueChangedByUser: value => root.setSetting("enableCloudGsync", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Steam Deck identity"); description: qsTr("Unlock Deck resolutions and 90 FPS · refreshes entitlements"); showDivider: false
                    DesktopSettingsToggle { checked: root.boolSetting("identifyAsSteamDeck", false); onValueChangedByUser: value => root.setSetting("identifyAsSteamDeck", value) }
                }
            }
        }
    }

    Component {
        id: audioPage
        Column {
            width: contentFlick.width; spacing: 14
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Output device"); description: qsTr("The native streamer follows your operating system output"); value: qsTr("SYSTEM DEFAULT") }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Game volume"); description: qsTr("Use the system mixer or the game's own audio settings"); value: qsTr("SYSTEM MIXER"); showDivider: false }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 56; title: qsTr("Microphone upstream"); description: qsTr("Microphone upstream is unavailable for NVST sessions"); value: qsTr("UNAVAILABLE"); showDivider: false }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Item { width: parent.width; height: 30
                    Text { anchors.left: parent.left; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: qsTr("Audio format and channel count are negotiated with the active GeForce NOW session."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; elide: Text.ElideRight }
                }
            }
        }
    }

    Component {
        id: controllersPage
        Column {
            width: contentFlick.width; spacing: 14
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Connected controllers"); description: qsTr("Controllers are detected through SDL without restarting OpenNOW"); value: qsTr("%1 CONNECTED").arg(AppController.controllerCount); showDivider: false
                    DesktopSettingsToggle { checked: root.boolSetting("controllerMode", true); onValueChangedByUser: value => root.setSetting("controllerMode", value) }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Switch to console mode when a pad wakes up"); description: qsTr("Open the gamepad-first shell when a controller becomes active")
                    DesktopSettingsToggle { checked: root.boolSetting("switchToConsoleOnPad", true); onValueChangedByUser: value => root.setSetting("switchToConsoleOnPad", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Return to desktop on pointer input"); description: qsTr("Mouse movement leaves console mode after the current input hold")
                    DesktopSettingsToggle { checked: root.boolSetting("leaveConsoleOnPointer", true); onValueChangedByUser: value => root.setSetting("leaveConsoleOnPointer", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Gyroscope controls"); description: qsTr("Forward supported motion input to the active stream")
                    DesktopSettingsToggle { checked: root.boolSetting("enableGyroscopeControls", false); onValueChangedByUser: value => root.setSetting("enableGyroscopeControls", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 56; title: qsTr("Steam controller compatibility"); description: qsTr("Use the compatibility path for Steam-managed controllers"); showDivider: false
                    DesktopSettingsToggle { checked: root.boolSetting("steamControllerCompatibilityMode", false); onValueChangedByUser: value => root.setSetting("steamControllerCompatibilityMode", value) }
                }
            }
            DesktopSettingsPanel { width: parent.width; padding: 18
                Item { width: parent.width; height: 30
                    Text { anchors.left: parent.left; anchors.right: inputTest.left; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: qsTr("OpenNOW forwards controller input to the native streamer; game bindings remain in the game."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; elide: Text.ElideRight }
                    DesktopSettingsButton { id: inputTest; anchors.right: parent.right; text: qsTr("Controller order"); compact: true; onClicked: AppController.navigate("joining") }
                }
            }
        }
    }

    Component {
        id: networkPage
        Column {
            width: contentFlick.width; spacing: 14
            Component.onCompleted: {
                ShellStore.refreshRegions()
                ShellStore.pingRegions()
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: "Server region"; description: ShellStore.regions.length ? qsTr("%1 streaming regions from your account").arg(ShellStore.regions.length) : qsTr("Sign in to discover available regions")
                    Item {
                        width: signalBars.width + regionMs.implicitWidth + 14
                        height: DesktopTokens.controlHeight
                        Item {
                            id: signalBars
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            width: DesktopTokens.px(16)
                            height: DesktopTokens.px(13)
                            Repeater {
                                model: 3
                                Rectangle {
                                    required property int index
                                    readonly property bool lit: root.pingBarsLit(root.currentRegionPing()) > index
                                    x: index * (DesktopTokens.px(4) + DesktopTokens.px(2))
                                    y: parent.height - height
                                    width: DesktopTokens.px(4)
                                    height: [DesktopTokens.px(5), DesktopTokens.px(9), DesktopTokens.px(13)][index]
                                    radius: DesktopTokens.px(1)
                                    color: lit ? root.pingRankColor(root.currentRegionPing()) : Qt.rgba(1,1,1,0.16)
                                }
                            }
                        }
                        Text {
                            id: regionMs
                            anchors.left: signalBars.right
                            anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.currentRegionPing() === null ? "—" : root.currentRegionPing() + " ms"
                            color: root.pingRankColor(root.currentRegionPing())
                            font.family: Theme.monoFont
                            font.pixelSize: DesktopTokens.microSize
                            font.weight: Font.Bold
                        }
                    }
                    DesktopSettingsButton { id: networkRegion; menu: true; text: root.currentRegionLabel(); compact: true; onClicked: root.openChoices(networkRegion, "region", root.regionChoiceItems()) }
                    DesktopSettingsButton { text: ShellStore.regionPingRequestId === "" ? qsTr("Ping regions") : qsTr("Pinging…"); compact: true; onClicked: ShellStore.pingRegions() }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: "Bandwidth ceiling"; description: "Hard cap for the stream · useful on shared or metered lines"
                    DesktopSettingsSlider { from: 10; to: 200; stepSize: 5; value: Number(root.valueSetting("maxBitrateMbps", 75)); suffix: " Mbps"; onCommitted: value => root.setSetting("maxBitrateMbps", Math.round(value)) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: "Proxy"; description: "Applies to API calls only · the stream always goes direct"; showDivider: false
                    TextField { width: 280; height: 34; text: String(root.valueSetting("sessionProxyUrl", "")); placeholderText: qsTr("http://proxy.example:8080"); color: Theme.label; placeholderTextColor: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.microSize; onEditingFinished: root.setSetting("sessionProxyUrl", text); background: Rectangle { radius: 9; color: Qt.rgba(1,1,1,0.04); border.width: 1; border.color: parent.activeFocus ? DesktopTokens.focus : Qt.rgba(1,1,1,0.12) } }
                    DesktopSettingsToggle { checked: root.boolSetting("sessionProxyEnabled", false); onValueChangedByUser: value => root.setSetting("sessionProxyEnabled", value) }
                }
            }
            DesktopSettingsPanel { width: parent.width; padding: 18
                Item { width: parent.width; height: 30
                    Text { anchors.left: parent.left; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: qsTr("Region, bitrate and proxy apply to API calls and session setup; media always streams direct."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; elide: Text.ElideRight }
                }
            }
        }
    }

    Component {
        id: interfacePage
        Column {
            width: contentFlick.width; spacing: 10
            DesktopSettingsPanel {
                width: parent.width; padding: 16
                DesktopSettingsRow { width: parent.width; rowHeight: 52; title: "Language"; description: "Community translated through Crowdin · 14 languages"; showDivider: false
                    DesktopSettingsButton { id: languageButton; text: String(root.valueSetting("appLanguage", "en")).toLowerCase() === "en" ? "ENGLISH (UK)" : String(root.valueSetting("appLanguage", "system")).toUpperCase(); compact: true; onClicked: root.openChoices(languageButton, "appLanguage", root.choices([{kind:"choice",label:"System",value:"system"},{kind:"choice",label:"English (UK)",value:"en"},{kind:"choice",label:"Nederlands",value:"nl"},{kind:"choice",label:"Deutsch",value:"de"},{kind:"choice",label:"Français",value:"fr"},{kind:"choice",label:"Türkçe",value:"tr"}])) }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 16
                DesktopSettingsRow { width: parent.width; rowHeight: 52; title: "Sidebar"; description: "Collapsed shows icons only and expands on hover · Ctrl B toggles"
                    DesktopSettingsToggle { checked: root.boolSetting("desktopRailCollapsed", true); onValueChangedByUser: value => root.setSetting("desktopRailCollapsed", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 52; title: "Grid density"; description: "Cover size in Library and Store"
                    DesktopSettingsSegmented { options: ["DENSE","COMFORTABLE","LARGE"]; optionWidth: 74; selectedIndex: Number(root.valueSetting("posterSizeScale", 1.05)) < 1 ? 0 : Number(root.valueSetting("posterSizeScale", 1.05)) > 1.1 ? 2 : 1; onSelected: (index,value) => root.setSetting("posterSizeScale", value === "DENSE" ? 0.9 : value === "LARGE" ? 1.25 : 1.05) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 52; title: "Reduce motion"; description: "Cuts parallax and cover animations · follows your OS by default"
                    DesktopSettingsToggle { checked: root.boolSetting("reducedMotion", false); onValueChangedByUser: value => root.setSetting("reducedMotion", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 52; title: qsTr("Translucent interface"); description: qsTr("Use translucent shell surfaces when supported"); showDivider: false
                    DesktopSettingsToggle { checked: root.boolSetting("translucentUI", false); onValueChangedByUser: value => root.setSetting("translucentUI", value) }
                }
            }
        }
    }

    Component {
        id: themesPage
        Column {
            id: themesPageRoot
            width: contentFlick.width; spacing: 14
            readonly property var activeTheme: root.themeMeta(String(ShellStore.settings.themePack || "nocturne"))
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Row {
                    width: parent.width
                    height: 114
                    spacing: 18
                    Rectangle { width: 212; height: 100; radius: 12; color: Theme.shell; border.width: 1; border.color: themesPageRoot.activeTheme.accent
                        Row { x: 50; y: 34; spacing: 7
                            Repeater { model: [themesPageRoot.activeTheme.accent, Qt.darker(themesPageRoot.activeTheme.accent, 1.35), Qt.lighter(themesPageRoot.activeTheme.accent, 1.4)]; delegate: Rectangle { required property var modelData; width: 44; height: 56; radius: 6; color: modelData } }
                        }
                    }
                    Column {
                        width: parent.width - 212 - 158 - 36
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 7
                        Text { text: qsTr("ACTIVE THEME"); color: DesktopTokens.focus; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold; font.letterSpacing: 1 }
                        Text { text: root.themeMeta(String(root.valueSetting("themePack", "nocturne"))).name; color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(24); font.weight: Font.Black }
                        Text { width: parent.width; wrapMode: Text.WordWrap; text: root.themeMeta(String(root.valueSetting("themePack", "nocturne"))).blurb; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                    }
                    Column { width: 144; anchors.verticalCenter: parent.verticalCenter; spacing: 8
                        DesktopSettingsButton { width: 144; text: qsTr("Theme store"); primary: true; onClicked: AppController.navigate("theme-store") }
                    }
                }
            }
        }
    }

    Component {
        id: consolePage
        Column {
            width: contentFlick.width; spacing: 14
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                Item { width: parent.width; height: Math.max(120, consoleCopy.implicitHeight + 48)
                    Rectangle { id: consolePreview
                        x: 0; y: (parent.height - height) / 2
                        width: Math.min(212, Math.max(160, parent.width * 0.26)); height: 120
                        radius: 12; color: Theme.shell; border.width: 1; border.color: Qt.rgba(0.6,0.5,1,0.25)
                        Row { x: (parent.width - 164) / 2; y: 34; spacing: 7; Repeater { model: [Theme.focus, Theme.violet, Theme.mint]; delegate: Rectangle { required property var modelData; width: 50; height: 56; radius: 7; color: modelData } } }
                        Text { x: 12; y: 96; text: "A  PLAY   B  BACK"; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold }
                    }
                    Column {
                        id: consoleCopy
                        anchors.left: consolePreview.right
                        anchors.leftMargin: 18
                        anchors.right: consoleActions.left
                        anchors.rightMargin: 18
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 9
                        Row { spacing: 10; Text { text: qsTr("One app, two shells"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black } Text { text: qsTr("GAMEPAD READY"); color: DesktopTokens.green; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold; anchors.verticalCenter: parent.verticalCenter } }
                        Text { width: parent.width; wrapMode: Text.WordWrap; text: qsTr("Console mode swaps the desktop chrome for big art, focus rings and glyph hints. Same session, same settings, same themes — nothing restarts and a running stream keeps going."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; lineHeight: 1.45 }
                    }
                    Column {
                        id: consoleActions
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: 158; spacing: 8
                        DesktopSettingsButton { width: 158; text: ShellStore.consoleSurfaceRequestId === "" ? qsTr("Switch now") : qsTr("Switching…"); suffix: "F10"; primary: true; enabled: ShellStore.consoleSurfaceRequestId === ""; onClicked: ShellStore.requestConsoleSurface(true) }
                    }
                }
            }
            DesktopSettingsPanel {
                width: parent.width; padding: 18
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: "Start in console mode"; description: "Remember this choice for the next time OpenNOW launches"
                    DesktopSettingsToggle {
                        checked: root.boolSetting("launchInConsoleMode", false)
                        onValueChangedByUser: value => root.setSetting("launchInConsoleMode", value)
                    }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: "Enter console mode when a gamepad is the only input"; description: "Ignored while a mouse has moved in the last 30 seconds"
                    DesktopSettingsToggle { checked: root.boolSetting("switchToConsoleOnPad", true); onValueChangedByUser: value => root.setSetting("switchToConsoleOnPad", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: "Leave console mode on keyboard or mouse input"; description: "Keeps your place in the grid when the shell swaps"
                    DesktopSettingsToggle { checked: root.boolSetting("leaveConsoleOnPointer", true); onValueChangedByUser: value => root.setSetting("leaveConsoleOnPointer", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: "Go fullscreen in console mode"; description: "Recommended on a TV · hides the window chrome entirely"
                    DesktopSettingsToggle { checked: root.boolSetting("autoFullScreen", false); onValueChangedByUser: value => root.setSetting("autoFullScreen", value) }
                }
                DesktopSettingsRow { width: parent.width; rowHeight: 62; title: qsTr("Controller profile picker"); description: qsTr("Choose a saved profile when console mode starts"); showDivider: false
                    DesktopSettingsToggle { checked: root.boolSetting("consoleProfilePickerOnLaunch", true); onValueChangedByUser: value => root.setSetting("consoleProfilePickerOnLaunch", value) }
                }
            }
            DesktopSettingsPanel { width: parent.width; padding: 18
                Item { width: parent.width; height: 30
                    Text { anchors.left: parent.left; anchors.right: cli.left; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: "●   Launch OpenNOW with --console to boot straight into the gamepad shell. That is the flag Steam Deck and TV boxes should use in their launcher entry."; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; elide: Text.ElideRight }
                    Rectangle { id: cli; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; width: 132; height: 28; radius: 8; color: Qt.rgba(1,1,1,0.07); border.width: 1; border.color: Qt.rgba(1,1,1,0.12); Text { anchors.centerIn: parent; text: "opennow --console"; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold } }
                }
            }
        }
    }

    Component {
        id: shortcutsPage
        Column {
            id: shortcutsPageRoot
            width: contentFlick.width; spacing: 14
            property string shortcutQuery: ""

            function allShortcutGroups() {
                return [
                    {h:"APP", rows:[{l:"Command palette",k:"Ctrl  K"},{l:"Search this page",k:"/"},{l:"Collapse or expand the sidebar",k:"Ctrl  B"},{l:"Switch to console mode",k:"F10"},{l:"Settings",k:"Ctrl  ,"},{l:"Quit OpenNOW",k:"Ctrl  Q"}]},
                    {h:"IN STREAM", rows:[{l:"Session menu",k:"Esc"},{l:"Stats overlay",k:String(root.valueSetting("shortcutToggleStats","Ctrl+N"))},{l:"Toggle fullscreen",k:String(root.valueSetting("shortcutToggleFullscreen","F11"))},{l:"Grab or release the mouse",k:String(root.valueSetting("shortcutTogglePointerLock","F8"))},{l:"Screenshot the stream",k:String(root.valueSetting("shortcutScreenshot","Ctrl+F11"))},{l:"End the session",k:String(root.valueSetting("shortcutStopStream","Ctrl+Shift+Q"))}]},
                    {h:"LIBRARY AND STORE", rows:[{l:"Move through covers",k:"Arrows"},{l:"Play or resume",k:"Enter"},{l:"Game details",k:"Space"},{l:"Toggle favourite",k:"F"},{l:"Context menu",k:"Shift  F10"}]},
                    {h:"GAMEPAD · CONSOLE MODE", rows:[{l:"Select · back",k:"A · B"},{l:"Details · favourite",k:"X · Y"},{l:"Switch tab",k:"LB · RB"},{l:"Stats overlay",k:"Guide"}]}
                ]
            }

            function shortcutGroups() {
                const query = shortcutsPageRoot.shortcutQuery.trim().toLocaleLowerCase()
                const groups = shortcutsPageRoot.allShortcutGroups()
                if (query === "")
                    return groups
                const result = []
                for (let i = 0; i < groups.length; ++i) {
                    const rows = []
                    for (let j = 0; j < groups[i].rows.length; ++j) {
                        const row = groups[i].rows[j]
                        if (String(row.l).toLocaleLowerCase().indexOf(query) >= 0
                                || String(row.k).toLocaleLowerCase().indexOf(query) >= 0
                                || String(groups[i].h).toLocaleLowerCase().indexOf(query) >= 0)
                            rows.push(row)
                    }
                    if (rows.length > 0)
                        result.push({h: groups[i].h, rows: rows})
                }
                return result
            }

            DesktopSettingsPanel {
                width: parent.width; padding: 16
                Row { width: parent.width; height: 34; spacing: 12
                    TextField { width: parent.width - 262; height: 34; placeholderText: "⌕  Search a command or press a key to find its binding"; color: Theme.label; placeholderTextColor: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; onTextChanged: shortcutsPageRoot.shortcutQuery = text; background: Rectangle { radius: 9; color: Qt.rgba(1,1,1,0.04); border.width: 1; border.color: parent.activeFocus ? DesktopTokens.focus : Qt.rgba(1,1,1,0.10) } }
                    Text { width: 148; height: 34; text: shortcutsPageRoot.shortcutQuery === "" ? qsTr("CURRENT BINDINGS") : qsTr("%1 MATCHES").arg(shortcutsPageRoot.shortcutGroups().reduce((count, group) => count + group.rows.length, 0)); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                    DesktopSettingsButton { width: 90; text: "Reset all"; compact: true; onClicked: ShellStore.resetSettings() }
                }
            }
            GridLayout {
                width: parent.width; columns: 2; columnSpacing: 14; rowSpacing: 14
                Repeater {
                    model: shortcutsPageRoot.shortcutGroups()
                    delegate: DesktopSettingsPanel {
                        required property var modelData
                        Layout.fillWidth: true; Layout.preferredHeight: modelData.rows.length * 40 + 48; padding: 16
                        Text { text: modelData.h; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold; font.letterSpacing: 1 }
                        Repeater { model: modelData.rows
                            delegate: Item { required property var modelData; width: parent.width; height: 40
                                Text { anchors.left: parent.left; anchors.right: keyCap.left; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData.l; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; elide: Text.ElideRight }
                                Rectangle { id: keyCap; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; width: Math.max(56, keyCapText.implicitWidth + 20); height: 28; radius: 8; color: Qt.rgba(1,1,1,0.07); border.width: 1; border.color: Qt.rgba(1,1,1,0.12)
                                    Text { id: keyCapText; anchors.centerIn: parent; text: modelData.k; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.microSize; font.weight: Font.Bold }
                                }
                            }
                        }
                    }
                }
            }
            Text {
                visible: shortcutsPageRoot.shortcutQuery !== "" && shortcutsPageRoot.shortcutGroups().length === 0
                width: parent.width; horizontalAlignment: Text.AlignHCenter
                text: qsTr("No bindings match “%1”.").arg(shortcutsPageRoot.shortcutQuery)
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
        }
    }

    Component {
        id: aboutPage
        Column {
            width: contentFlick.width; spacing: 14
            DesktopSettingsPanel { width: parent.width; padding: 18
                Item { width: parent.width; height: 54
                    Row { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; spacing: 16
                        Image { width: 44; height: 24; anchors.verticalCenter: parent.verticalCenter; source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"; fillMode: Image.PreserveAspectFit; smooth: false }
                        Column { anchors.verticalCenter: parent.verticalCenter; spacing: 3; Text { text: "OpenNOW " + String(ShellStore.updaterState.currentVersion || qsTr("unknown")); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(22); font.weight: Font.Black } Text { text: qsTr("Qt Quick · native streaming runtime"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.microSize; font.weight: Font.Bold } }
                        Text { anchors.verticalCenter: parent.verticalCenter; text: String(ShellStore.updaterState.status || qsTr("idle")).toUpperCase(); color: ShellStore.updaterState.status === "available" ? DesktopTokens.amber : DesktopTokens.green; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold }
                    }
                    Row { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 10
                        DesktopSettingsButton { id: updateChannelButton; menu: true; text: String(root.valueSetting("updateChannel","stable")).toUpperCase() + " CHANNEL"; onClicked: root.openChoices(updateChannelButton,"updateChannel",root.choices([{kind:"choice",label:"Stable",value:"stable"},{kind:"choice",label:"Nightly",value:"nightly"}])) }
                        DesktopSettingsButton { text: "Check for updates"; primary: true; onClicked: ShellStore.checkForUpdates() }
                    }
                }
            }
            Row { width: parent.width; spacing: 14
                DesktopSettingsPanel { width: parent.width - 340; padding: 18
                    Text { width: parent.width; text: ShellStore.releaseHighlights.title || qsTr("Release notes"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold }
                    Text { width: parent.width; height: 178; text: ShellStore.releaseHighlights.bodyMarkdown || ShellStore.updaterState.message || qsTr("Check for updates to load verified release information from GitHub."); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; wrapMode: Text.WordWrap; elide: Text.ElideRight }
                }
                DesktopSettingsPanel { width: 326; padding: 18
                    Text { text: "Project"; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold }
                    Repeater { model: root.projectLinks()
                        delegate: ItemDelegate { required property var modelData; width: parent.width; height: 54; padding: 0; hoverEnabled: true
                            onClicked: root.runProjectLink(modelData)
                            background: Rectangle { radius: 10; color: parent.hovered || parent.activeFocus ? Qt.rgba(1,1,1,0.05) : "transparent" }
                            contentItem: Item {
                                Text { anchors.left: parent.left; anchors.leftMargin: 4; anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                                Text { anchors.right: parent.right; anchors.rightMargin: 4; anchors.verticalCenter: parent.verticalCenter; text: modelData.hint; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: modelData.hint.length > 2 ? 8 : 14; font.weight: Font.Bold }
                            }
                            Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 1; color: Qt.rgba(1,1,1,0.06) }
                        }
                    }
                }
            }
            DesktopSettingsPanel { width: parent.width; padding: 18
                Text { width: parent.width; height: 40; verticalAlignment: Text.AlignVCenter; text: "●   OpenNOW is an independent client. Not affiliated with, endorsed by or supported by NVIDIA. GeForce NOW is a trademark of NVIDIA Corporation. You bring your own account and your own subscription."; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; wrapMode: Text.WordWrap }
            }
        }
    }
}

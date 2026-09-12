import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopSettingsScreen"
    anchors.fill: parent
    clip: true

    property int selectedSection: 3
    property string searchQuery: ""
    property bool advancedOpen: false
    // Isolated visual acceptance only; never selects alternate production UI.
    property string acceptancePanel: ""
    readonly property var acceptancePanels: ({stats:statsSettingsPage, audio:audioPage,
        interface:interfacePage, console:consolePage, shortcuts:shortcutsPage,
        controllers:controllersPage, subscription:subscriptionPage, recording:recordingPage})
    readonly property bool compactNavigation: width < 1050
    readonly property int selectedGroup: sections.findIndex(section => section.page ===
        ([0,1,2].indexOf(selectedSection) >= 0 ? 0 : selectedSection === 10 ? 5 : selectedSection === 7 ? 8 : selectedSection))
    onSelectedSectionChanged: { advancedOpen = false }
    signal requestConsoleMode(bool enabled)

    readonly property var sections: [
        {label: qsTr("Stream"), detail: qsTr("Picture, codec, bitrate"), icon: "monitor", page: 3, keywords: "resolution fps hdr color audio stats overlay bitrate codec reflex backend gpu directx vulkan steam big picture launch gamepad fullscreen session ready persistent in-game graphics settings"},
        {label: qsTr("Audio"), detail: qsTr("Output and stream audio"), icon: "wave", page: 4, keywords: "sound audio volume output microphone"},
        {label: qsTr("Recording"), detail: qsTr("Capture, replay, shortcuts"), icon: "image", page: 12, keywords: "recording capture clip replay buffer memory duration folder resolution fps quality shortcuts F12"},
        {label: qsTr("Controls"), detail: qsTr("Pads, mouse, shortcuts"), icon: "controller", page: 5, keywords: "controller gyroscope steam sensitivity keyboard language shortcuts"},
        {label: qsTr("Look"), detail: qsTr("Theme, accent, layout"), icon: "palette", page: 8, keywords: "theme accent interface language scale motion console sidebar tiles"},
        {label: qsTr("Console mode"), detail: qsTr("Gamepad-first interface"), icon: "controller", page: 9, keywords: "console fullscreen gamepad startup"},
        {label: qsTr("Network"), detail: qsTr("Region, ping, proxy"), icon: "globe", page: 6, keywords: "server region ping proxy l4s"},
        {label: qsTr("Account"), detail: qsTr("NVIDIA, stores, privacy"), icon: "person", page: 0, keywords: "profile subscription stores steam epic xbox ubisoft battle gaijin privacy"},
        {label: qsTr("About"), detail: qsTr("Updates, diagnostics"), icon: "info", page: 11, keywords: "version release update diagnostics onboarding introduction replay setup restart"}
    ]
    readonly property var pageTitles: [qsTr("Account"), qsTr("Account"), qsTr("Account"), qsTr("Stream"), qsTr("Audio"), qsTr("Controls"), qsTr("Network"), qsTr("Look"), qsTr("Look"), qsTr("Console mode"), qsTr("Controls"), qsTr("About"), qsTr("Recording")]
    readonly property var pageComponents: [accountGroup, subscriptionPage, accountGroup, streamPage, audioPage, controlsGroup, networkPage, lookGroup, lookGroup, consolePage, controlsGroup, aboutPage, recordingPage]

    function matchesSection(section) {
        const query = searchQuery.trim().toLowerCase()
        return !query || (section.label + " " + section.detail + " " + section.keywords).toLowerCase().indexOf(query) >= 0
    }

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
        const n = String(name || "").trim().toLowerCase()
        if (/^(us|ca)\b|\b(usa|canada|north america)\b/.test(n))
            return qsTr("NORTH AMERICA")
        if (/^(eu|uk|tr)\b|\b(europe|united kingdom|sweden|netherlands|germany|france|poland|bulgaria|turkey|türkiye|london|frankfurt|amsterdam)\b/.test(n))
            return qsTr("EUROPE")
        if (/^(jp|kr|sg|au|tw|my|th)\b|\b(asia|japan|korea|taiwan|malaysia|thailand|australia|new zealand|tokyo|seoul|singapore|sydney|india)\b/.test(n))
            return qsTr("ASIA PACIFIC")
        if (/^br\b|\b(latam|south america|brazil|sao|são|chile|colombia|uruguay)\b/.test(n))
            return qsTr("SOUTH AMERICA")
        if (/^me\b|\b(middle east|uae|saudi|riyadh)\b/.test(n))
            return qsTr("MIDDLE EAST")
        if (/\b(africa|johannesburg)\b/.test(n))
            return qsTr("AFRICA")
        return qsTr("OTHER")
    }

    function regionChoiceItems() {
        const items = [{ kind: "choice", label: qsTr("Automatic"), detail: qsTr("Lowest latency"), value: "" }]
        const groups = [qsTr("EUROPE"), qsTr("NORTH AMERICA"), qsTr("ASIA PACIFIC"),
                        qsTr("SOUTH AMERICA"), qsTr("MIDDLE EAST"), qsTr("AFRICA"), qsTr("OTHER")]
        const regions = (ShellStore.regions || []).slice().sort((a, b) => String(a.name).localeCompare(String(b.name)))
        for (const group of groups) {
            const members = regions.filter(region => root.regionGroup(region.name) === group)
            if (!members.length)
                continue
            items.push({ kind: "heading", label: group })
            for (const region of members) {
                const ping = root.regionPingMs(region.url)
                items.push({
                    kind: "choice",
                    label: region.name,
                    detail: ShellStore.regionPingBusy ? qsTr("Measuring…") : ping !== null ? (ping + " ms")
                        : ShellStore.regionPingResults[region.url] === null ? qsTr("No response") : qsTr("Not measured"),
                    detailColor: ping === null ? "" : root.pingRankColor(ping),
                    value: region.url
                })
            }
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
        return raw === undefined || raw === null || raw === "" || !Number.isFinite(n) ? null : n
    }

    function currentRegionPing() {
        const selected = String(root.valueSetting("region", ""))
        if (selected === "") {
            const measured = (ShellStore.regions || []).map(region => root.regionPingMs(region.url))
                .filter(ping => ping !== null)
            return measured.length ? Math.min.apply(null, measured) : null
        }
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
        return DesktopTokens.storeIconUrl(account.provider || account.label)
            || "qrc:/qt/qml/OpenNOW/res/icons/desktop-nav-store.svg"
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
        return ShellStore.resolutionItems()
    }

    Column {
        id: settingsRail
        x: DesktopTokens.px(24)
        y: DesktopTokens.px(18)
        width: root.compactNavigation ? root.width - x * 2 : DesktopTokens.px(272)
        spacing: DesktopTokens.px(14)

        TextField {
            id: settingsSearch
            objectName: "settingsSearch"
            width: parent.width
            height: DesktopTokens.px(44)
            placeholderText: qsTr("Search settings")
            text: root.searchQuery
            onTextEdited: root.searchQuery = text
            color: Theme.label
            placeholderTextColor: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            leftPadding: 42
            DesktopGlyph { x: 16; anchors.verticalCenter: parent.verticalCenter; width: 16; height: 16; icon: "desktop-search.svg" }
            background: Rectangle { radius: 10; color: Theme.glass; border.width: 1; border.color: settingsSearch.activeFocus ? Theme.focus : Theme.seam }
            onAccepted: {
                for (let i = 0; i < root.sections.length; ++i) {
                    if (root.matchesSection(root.sections[i])) {
                        root.selectedSection = root.sections[i].page
                        break
                    }
                }
            }
        }
        Flickable {
            width: parent.width
            height: root.compactNavigation ? DesktopTokens.px(52) : Math.max(0, root.height - settingsRail.y - settingsSearch.height - 36)
            contentWidth: root.compactNavigation ? navigation.implicitWidth : width
            contentHeight: navigation.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Flow {
                id: navigation
                width: root.compactNavigation ? implicitWidth : parent.width
                flow: root.compactNavigation ? Flow.TopToBottom : Flow.LeftToRight
                height: root.compactNavigation ? DesktopTokens.px(52) : implicitHeight
                spacing: DesktopTokens.px(6)
                Repeater {
                    model: root.sections
                    delegate: Button {
                        required property var modelData
                        required property int index
                        visible: root.matchesSection(modelData)
                        width: root.compactNavigation ? DesktopTokens.px(130) : settingsRail.width
                        height: DesktopTokens.px(root.compactNavigation ? 48 : 64)
                        padding: 12
                        hoverEnabled: true
                        onClicked: root.selectedSection = modelData.page
                        background: Rectangle {
                            radius: 16
                            color: root.selectedGroup === index ? DesktopTokens.raisedStrong : parent.hovered ? DesktopTokens.raised : "transparent"
                            border.width: parent.activeFocus ? 2 : 0
                            border.color: Theme.focus
                        }
                        contentItem: RowLayout {
                            spacing: 14
                            Rectangle {
                                visible: !root.compactNavigation
                                Layout.preferredWidth: 40; Layout.preferredHeight: 40
                                radius: 12; color: root.selectedGroup === index ? Theme.focus : DesktopTokens.raised
                                DesktopSettingsIcon { anchors.centerIn: parent; width: 20; height: 20; glyph: modelData.icon; ink: root.selectedGroup === index ? Theme.focusText : Theme.label }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text { Layout.fillWidth: true; text: modelData.label; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: root.selectedGroup === index ? Font.ExtraBold : Font.Bold; elide: Text.ElideRight }
                                Text { visible: !root.compactNavigation; Layout.fillWidth: true; text: modelData.detail; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; elide: Text.ElideRight }
                            }
                        }
                    }
                }
            }
        }
    }

    Item {
        id: contentLane
        x: root.compactNavigation ? settingsRail.x : settingsRail.x + settingsRail.width + DesktopTokens.px(20)
        y: root.compactNavigation ? settingsRail.y + settingsRail.height + DesktopTokens.px(16) : settingsRail.y
        width: root.width - x - DesktopTokens.px(24)
        height: root.height - y - DesktopTokens.px(18)
        Flickable {
            id: contentFlick
            anchors.fill: parent
            contentWidth: width
            contentHeight: pageLoader.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            Loader {
                id: pageLoader
                objectName: "settingsPageLoader"
                width: contentFlick.width
                sourceComponent: SmokeTestMode && root.acceptancePanel !== ""
                    ? root.acceptancePanels[root.acceptancePanel] : root.pageComponents[root.selectedSection]
                opacity: sectionEntrance.pageOpacity
                transform: Translate { y: sectionEntrance.offset }
                PageEntrance { id: sectionEntrance; objectName: "settingsPageEntrance" }
                onLoaded: { contentFlick.contentY = 0; sectionEntrance.restart() }
            }
        }
    }



    Connections {
        target: ShellStore
        function onSubscriptionChanged() { root.clampFpsToEntitlement() }
    }

    Component {
        id: accountGroup
        DesktopSettingsAccountPage {
            availableWidth: contentFlick.width
            settingsScreen: root
            profilePageComponent: profilePage
            subscriptionPageComponent: subscriptionPage
            storesPageComponent: storesPage
        }
    }
    Component {
        id: controlsGroup
        DesktopSettingsControlsPage {
            availableWidth: contentFlick.width
            settingsScreen: root
            controllersPageComponent: controllersPage
            shortcutsPageComponent: shortcutsPage
        }
    }

    Component {
        id: lookGroup
        DesktopSettingsLookPage {
            availableWidth: contentFlick.width
            settingsScreen: root
            statsSettingsPageComponent: statsSettingsPage
            interfacePageComponent: interfacePage
        }
    }

    Component {
        id: statsSettingsPage
        DesktopSettingsStatsPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: profilePage
        DesktopSettingsProfilePage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: subscriptionPage
        DesktopSettingsSubscriptionPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: storesPage
        DesktopSettingsStoresPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: streamPage
        DesktopSettingsStreamPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: audioPage
        DesktopSettingsAudioPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: controllersPage
        DesktopSettingsControllersPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: networkPage
        DesktopSettingsNetworkPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: interfacePage
        DesktopSettingsInterfacePage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }


    Component {
        id: consolePage
        DesktopSettingsConsolePage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: shortcutsPage
        DesktopSettingsShortcutsPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: recordingPage
        DesktopSettingsRecordingPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

    Component {
        id: aboutPage
        DesktopSettingsAboutPage {
            availableWidth: contentFlick.width
            settingsScreen: root
        }
    }

}

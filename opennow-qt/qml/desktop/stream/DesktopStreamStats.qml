pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import OpenNOW

Item {
    id: root
    objectName: "desktopStreamStats"
    property bool expanded: false
    property bool pointerLocked: false
    property var frameGenerationStats: ({})
    enabled: !pointerLocked
    signal cycleRequested()
    signal copyRequested()
    signal closeRequested()
    readonly property var live: ShellStore.streamer || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || session.streamProfile || ShellStore.runtimeStreamProfile || ({})
    readonly property real overlayScale: Math.max(0.85, Math.min(1.5, Number(ShellStore.settings.statsOverlayScale || 1)))
    readonly property real inset: 24
    readonly property string position: String(ShellStore.settings.statsOverlayPosition || "top-right")
    readonly property bool rightAligned: position.endsWith("right")
    readonly property bool bottomAligned: position.startsWith("bottom")
    readonly property real topRightInset: !visible || bottomAligned ? inset
        : expanded ? (panel.x + panel.width > width - 408 ? panel.y + panel.height + 12 : inset)
        : Math.max(compact.x + compact.width > width - 408 ? compact.y + compact.height + 12 : inset,
            clockPill.visible && clockPill.x + clockPill.width > width - 408 ? clockPill.y + clockPill.height + 12 : inset)
    readonly property color surface: Qt.rgba(14 / 255, 16 / 255, 24 / 255,
        Math.max(0.4, Math.min(1, Number(ShellStore.settings.statsOverlayOpacity || 85) / 100)))
    readonly property bool degraded: telemetryActive && read("packetLossPercent") > 0
    readonly property bool healthKnown: telemetryActive && read("packetLossPercent") !== null
    readonly property color accent: degraded ? "#F5A623" : "#6EE7B7"
    readonly property color statusColor: !healthKnown || degraded ? "#F5A623" : "#1DB954"
    readonly property color metricColor: degraded ? accent : "white"
    readonly property string healthText: !telemetryActive ? qsTr("Waiting for stream")
        : degraded ? qsTr("Connection unstable") : healthKnown ? qsTr("Stream healthy") : qsTr("Stream statistics")
    readonly property real allocatedBitrateMbps: Math.max(0, numeric(ShellStore.runtimeStreamProfile.maxBitrateMbps)
        || numeric(profile.maxBitrateMbps) || numeric(ShellStore.settings.maxBitrateMbps) || 0)
    readonly property real bitrateUsage: allocatedBitrateMbps > 0 && read("bitrateMbps") !== null
        ? Math.max(0, Math.min(1, read("bitrateMbps") / allocatedBitrateMbps)) : 0
    readonly property string toggleShortcut: String(ShellStore.settings.shortcutToggleStats || "Ctrl+N")
    readonly property var heroCards: ["Fps", "Ping", "Latency"].map(key => cards.find(card => card.key === key)).filter(card => card !== undefined)
    readonly property var ledgerCards: cards.filter(card => ["Jitter", "Drops", "PacketLoss", "Decode", "LocalOutputFps"].includes(card.key)
        && (card.key !== "Drops" || card.field === "videoDropCount" || card.value > 0))
    readonly property var featureBadges: {
        const badges = []
        if (shown("Video") && (profile.enableHdr === true || profile.hdr === true)) badges.push({text:"HDR", ink:"#C6A46A"})
        if (frameGenerationEnabled) badges.push({text:qsTr("FRAME GEN 2×"), ink:"#F5A623"})
        if (shown("Video") && Qt.platform.os === "osx" && ShellStore.settings.upscaling === "metalfx") badges.push({text:"METALFX", ink:"#7FD4FF"})
        return badges
    }
    readonly property bool telemetryActive: live.status === "streaming"
    readonly property bool frameGenerationEnabled: String(ShellStore.settings.frameGeneration || "off") === "2x"
    readonly property var cards: metricCards()
    readonly property var compactMetrics: compactItems()
    property var history: ({})
    property double nowMs: Date.now()
    function shown(key) { return ShellStore.settings["statsShow" + key] !== false }
    function numeric(value) {
        return value === undefined || value === null || value === "" || !Number.isFinite(Number(value)) ? null : Number(value)
    }
    function read(key) {
        if (!telemetryActive) return null
        const drops = ShellStore.streamDropCounts
        return numeric(drops[key] !== undefined ? drops[key] : live[key])
    }
    function frameGenerationOutputFps() { return numeric(frameGenerationStats.outputFps) }
    function frameGenerationState() {
        switch (String(frameGenerationStats.status || "unavailable")) {
        case "off": return qsTr("Off")
        case "warming-up": return qsTr("Warming up")
        case "active": return qsTr("Active")
        case "display-refresh": return qsTr("Display refresh")
        case "source-rate-limit": return qsTr("120 FPS generation limit")
        case "hdr-unavailable": return qsTr("Unavailable with HDR")
        case "overloaded": return qsTr("Overloaded")
        case "discontinuity": return qsTr("Discontinuity")
        default: return qsTr("Unavailable")
        }
    }
    function sample(card) {
        return card.field === "frameGenerationOutputFps"
            ? frameGenerationOutputFps() : read(card.field)
    }
    function format(value, decimals) { return numeric(value) === null ? qsTr("N/A") : Number(value).toFixed(decimals || 0) }
    function elapsedText() {
        const start = Number(ShellStore.streamStartedAtMs || 0)
        const total = start > 0 ? Math.floor(Math.max(0, nowMs - start) / 1000) : 0
        return Math.floor(total / 3600) + ":" + String(Math.floor(total / 60) % 60).padStart(2, "0") + ":" + String(total % 60).padStart(2, "0")
    }
    readonly property string region: String(session.regionName || session.region || session.serverRegionId
        || (typeof session.serverLocation === "string" ? session.serverLocation : "")
        || session.zone || qsTr("Region unavailable"))
    readonly property string rig: String(session.rigName || session.gpuName || session.gpuType || "")
    readonly property string sessionDescription: [region, typeof session.serverLocation === "string" ? session.serverLocation : "", rig]
        .filter((value, index, values) => value && values.indexOf(value) === index).join(" · ")
    readonly property string videoText: {
        const parts = []
        if (live.codec || profile.codec) parts.push(String(live.codec || profile.codec).toUpperCase())
        const dimensions = String(profile.resolution || "").split("x")
        const w = Number(profile.width || dimensions[0] || live.outputWidth || 0), h = Number(profile.height || dimensions[1] || live.outputHeight || 0)
        if (w && h) parts.push(w + "×" + h)
        const colors = {"8bit_420":"8-bit 4:2:0", "8bit_444":"8-bit 4:4:4", "10bit_420":"10-bit 4:2:0", "10bit_444":"10-bit 4:4:4"}
        if (colors[profile.colorQuality]) parts.push(colors[profile.colorQuality])
        else {
            if (profile.bitDepth) parts.push(profile.bitDepth + "-bit")
            if (profile.chroma) parts.push(String(profile.chroma))
        }
        if (profile.enableHdr === true || profile.hdr === true) parts.push("HDR")
        return parts.length ? parts.join(" · ") : qsTr("Video format unavailable")
    }
    function metricCards() {
        const cards = [
            {key:"Ping", label:qsTr("PING"), value:read("pingMs"), unit:"ms", field:"pingMs"},
            {key:"Fps", label:qsTr("STREAM FPS"), value:read("framesPerSecond"), unit:"fps", field:"framesPerSecond"},
            {key:"Bitrate", label:qsTr("BITRATE"), value:read("bitrateMbps"), unit:"Mbps", field:"bitrateMbps", decimals:1},
            {key:"Jitter", label:qsTr("JITTER"), value:read("jitterMs"), unit:"ms", field:"jitterMs", decimals:1},
            {key:"Drops", label:qsTr("VIDEO DROPS"), value:read("videoDropCount"), unit:qsTr("frames"), field:"videoDropCount"},
            {key:"Drops", label:qsTr("AUDIO DISCARDED"), value:read("audioDiscardedMs"), unit:"ms", field:"audioDiscardedMs", decimals:1},
            {key:"Drops", label:qsTr("AUDIO QUEUE DROPS"), value:read("audioPacketDropCount"), unit:qsTr("packets / blocks"), field:"audioPacketDropCount"},
            {key:"Drops", label:qsTr("CALLBACK DROPS"), value:read("callbackDropCount"), unit:qsTr("callbacks"), field:"callbackDropCount"},
            {key:"PacketLoss", label:qsTr("PACKET LOSS"), value:read("packetLossPercent"), unit:"%", field:"packetLossPercent", decimals:1},
            {key:"Decode", label:qsTr("DECODE"), value:read("decodeTimeMs"), unit:"ms", field:"decodeTimeMs", decimals:1},
            {key:"Latency", label:qsTr("LATENCY"), value:read("latencyMs"), unit:"ms", field:"latencyMs"}
        ]
        if (read("otherQueueDropCount") > 0)
            cards.push({key:"Drops", label:qsTr("UNCLASSIFIED DROPS"), value:read("otherQueueDropCount"), unit:qsTr("items"), field:"otherQueueDropCount"})
        if (frameGenerationEnabled)
            cards.push({key:"LocalOutputFps", label:qsTr("LOCAL OUTPUT FPS"), value:frameGenerationOutputFps(), unit:"fps", field:"frameGenerationOutputFps"})
        return cards.filter(item => shown(item.key)
            && ((item.key !== "Decode" && item.key !== "Latency") || item.value !== null))
    }
    function compactItems() {
        const items = cards.map(item => ({text:item.label + " " + format(item.value, item.decimals) + " " + item.unit}))
        if (frameGenerationEnabled)
            items.push({text:qsTr("FRAME GENERATION") + " " + frameGenerationState()})
        if (shown("Video")) items.push({text:videoText})
        if (shown("Region")) items.push({text:region})
        return items
    }
    function report() {
        const lines = cards.map(item => item.label + ": " + format(item.value, item.decimals) + " " + item.unit)
        if (frameGenerationEnabled)
            lines.push(qsTr("FRAME GENERATION") + ": " + frameGenerationState())
        if (shown("Region")) lines.unshift(region + (rig ? " · " + rig : ""))
        if (shown("Video")) lines.push(videoText)
        if (shown("Clock")) lines.push(qsTr("Session: ") + elapsedText())
        return qsTr("Stream stats:") + "\n" + lines.join("\n")
    }
    function resetHistory() { history = ({}) }
    function sampleHistory() {
        const next = ({})
        for (const card of cards) {
            const values = (history[card.field] || []).slice(-59)
            values.push(sample(card)); next[card.field] = values
        }
        history = next
    }
    function ledgerDetail(card) {
        if (card.field === "jitterMs") {
            const samples = (history.jitterMs || []).filter(value => value !== null)
            return samples.length ? qsTr("max %1 · 60 s").arg(format(Math.max(...samples), 1)) : ""
        }
        if (card.field === "videoDropCount") return qsTr("session total")
        if (card.field === "decodeTimeMs") return telemetryActive ? String(live.mediaBackend || "") : ""
        return ""
    }
    onTelemetryActiveChanged: resetHistory()
    onVisibleChanged: resetHistory()
    Connections { target: ShellStore; function onStreamStartedAtMsChanged() { root.resetHistory() } }
    Timer {
        running: root.visible; repeat: true; interval: 1000
        onTriggered: {
            root.nowMs = Date.now()
            if (!root.telemetryActive) return
            root.sampleHistory()
        }
    }
    component Mono: Text {
        color: "white"
        font.family: Theme.monoFont
        font.pixelSize: 13
        font.weight: Font.DemiBold
        elide: Text.ElideRight
    }
    component Rule: Rectangle { height: 1; color: "#0FFFFFFF" }
    component Clock: Row {
        spacing: 6
        Image {
            anchors.verticalCenter: parent.verticalCenter
            source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-clock.svg"
            width: 12; height: 12; sourceSize: Qt.size(24, 24)
        }
        Mono { text: root.elapsedText() }
    }
    component Sparkline: Canvas {
        property var samples: []
        property color ink: root.accent
        width: 36; height: 12
        visible: root.shown("Graphs")
        onSamplesChanged: requestPaint()
        onInkChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            ctx.clearRect(0, 0, width, height)
            const valid = samples.filter(value => value !== null)
            if (valid.length < 2) return
            const low = Math.min(...valid), high = Math.max(...valid)
            ctx.strokeStyle = ink; ctx.lineWidth = 1.3; ctx.beginPath()
            let started = false
            for (let i = 0; i < samples.length; ++i) {
                if (samples[i] === null) { started = false; continue }
                const x = i * width / Math.max(1, samples.length - 1)
                const y = high === low ? height / 2 : height - 2 - (samples[i] - low) / (high - low) * (height - 4)
                if (started) ctx.lineTo(x, y); else ctx.moveTo(x, y)
                started = true
            }
            ctx.stroke()
        }
    }
    Rectangle {
        id: compact
        objectName: "compactStatsBar"
        visible: !root.expanded
        readonly property real contentScale: Math.min(root.overlayScale, Math.max(0.5, (root.width - root.inset * 2) / (compactRow.implicitWidth + 26)))
        width: (compactRow.implicitWidth + 26) * contentScale
        height: 36 * contentScale
        x: root.rightAligned ? root.width - width - root.inset : root.inset
        y: root.bottomAligned ? root.height - height - root.inset : root.inset
        radius: height / 2; color: root.surface
        border.color: Qt.rgba(root.accent.r, root.accent.g, root.accent.b, 0.22)
        Row {
            id: compactRow
            x: 13 * compact.contentScale; y: 7 * compact.contentScale
            scale: compact.contentScale; transformOrigin: Item.TopLeft
            height: 22; spacing: 10
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 7; height: 7; radius: 4; color: root.statusColor }
            Repeater {
                model: {
                    const metrics = []
                    if (root.shown("Fps")) metrics.push({value:root.format(root.read("framesPerSecond")), unit:"fps"})
                    if (root.shown("Ping")) metrics.push({value:root.format(root.read("pingMs")), unit:"ms"})
                    if (root.shown("Region")) metrics.push({value:root.region, unit:"", region:true})
                    if (root.shown("Video")) {
                        const h = Number(root.profile.height || String(root.profile.resolution || "").split("x")[1] || root.live.outputHeight || 0)
                        metrics.push({value:String(root.live.codec || root.profile.codec || root.format(null)).toUpperCase(), unit:h ? h + "p" : ""})
                    }
                    return metrics
                }
                delegate: Row {
                    id: compactMetric
                    required property var modelData
                    required property int index
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 10
                    Rectangle { visible: compactMetric.index > 0; width: 1; height: 14; anchors.verticalCenter: parent.verticalCenter; color: "#1AFFFFFF" }
                    Row {
                        spacing: 4
                        Image { visible: compactMetric.modelData.region === true; anchors.verticalCenter: parent.verticalCenter; width: 11; height: 11; sourceSize: Qt.size(22, 22); source: "qrc:/qt/qml/OpenNOW/res/icons/stats-globe.svg" }
                        Mono {
                            id: compactValue
                            text: compactMetric.modelData.value
                            width: Math.min(implicitWidth, compactMetric.modelData.region ? 180 : 100)
                            font.pixelSize: compactMetric.modelData.region ? 10.5 : 12.5
                            color: compactMetric.modelData.region ? "#B3FFFFFF" : root.metricColor
                        }
                        Mono { anchors.baseline: compactValue.baseline; text: compactMetric.modelData.unit; font.pixelSize: 10; color: "#80FFFFFF"; font.weight: Font.Medium }
                    }
                }
            }
            Repeater {
                model: root.featureBadges
                delegate: Rectangle {
                    id: featureBadge
                    required property var modelData
                    width: badge.implicitWidth + 16; height: 22; radius: 6
                    color: Qt.rgba(badge.color.r, badge.color.g, badge.color.b, 0.14)
                    Mono { id: badge; anchors.centerIn: parent; text: featureBadge.modelData.text; color: featureBadge.modelData.ink; font.pixelSize: 10; font.weight: Font.Bold; font.letterSpacing: 0.8 }
                }
            }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                KeyboardGlyph { shortcut: root.toggleShortcut; keySize: 20; ink: "white" }
                Text { anchors.verticalCenter: parent.verticalCenter; text: qsTr("more"); color: "#8CFFFFFF"; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold }
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.cycleRequested() }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Expand stream statistics")
        Accessible.onPressAction: root.cycleRequested()
    }
    Rectangle {
        id: panel
        objectName: "expandedStatsPanel"
        visible: root.expanded
        width: Math.min(root.width - root.inset * 2, 420 * root.overlayScale)
        height: Math.min(root.height - root.inset * 2, (panelContents.implicitHeight + 2) * root.overlayScale)
        x: root.rightAligned ? root.width - width - root.inset : root.inset
        y: root.bottomAligned ? root.height - height - root.inset : root.inset
        radius: 20 * root.overlayScale; color: root.surface
        border.color: Qt.rgba(root.accent.r, root.accent.g, root.accent.b, 0.18)
        Flickable {
            x: root.overlayScale; y: root.overlayScale
            width: panel.width / root.overlayScale - 2
            height: panel.height / root.overlayScale - 2
            scale: root.overlayScale; transformOrigin: Item.TopLeft
            contentWidth: width; contentHeight: panelContents.implicitHeight
            clip: true; boundsBehavior: Flickable.StopAtBounds
            Column {
                id: panelContents
                width: parent.width
                Item {
                    width: parent.width; height: 57
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 16; anchors.rightMargin: 16; anchors.topMargin: 14; anchors.bottomMargin: 10
                        spacing: 10
                        Rectangle { implicitWidth: 9; implicitHeight: 9; radius: 5; color: root.statusColor }
                        Column {
                            Layout.fillWidth: true; spacing: 2
                            Text { width: parent.width; text: root.healthText; elide: Text.ElideRight; color: root.degraded ? root.accent : "white"; font.family: Theme.bodyFont; font.pixelSize: 15; font.weight: Font.ExtraBold }
                            Mono { visible: root.shown("Region"); width: parent.width; text: root.sessionDescription; color: "#73FFFFFF"; font.pixelSize: 11; font.weight: Font.Medium }
                        }
                        Clock { visible: root.shown("Clock") }
                    }
                }
                RowLayout {
                    visible: root.heroCards.length > 0
                    width: parent.width - 32; x: 16; height: 69
                    spacing: 26
                    Repeater {
                        model: root.heroCards
                        delegate: Item {
                            id: hero
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true; Layout.preferredWidth: 111; Layout.fillHeight: true
                            Rectangle { visible: hero.index > 0; x: -14; y: 8; width: 1; height: 45; color: "#14FFFFFF" }
                            Row {
                                y: 6; spacing: 4
                                Mono { id: heroValue; text: root.format(hero.modelData.value); color: root.metricColor; font.pixelSize: 26 }
                                Mono { anchors.baseline: heroValue.baseline; text: hero.modelData.unit; color: "#80FFFFFF"; font.pixelSize: 11; font.weight: Font.Medium }
                            }
                            Mono { y: 43; text: hero.modelData.key === "Fps" ? qsTr("FPS") : hero.modelData.label; font.pixelSize: 10; font.letterSpacing: 0.95; color: "#80FFFFFF" }
                            Sparkline { x: parent.width - width; y: 43; samples: root.history[hero.modelData.field] || [] }
                        }
                    }
                }
                Item {
                    visible: root.shown("Bitrate")
                    width: parent.width; height: 41
                    Mono { x: 16; y: 2; text: qsTr("BITRATE"); font.pixelSize: 10; font.letterSpacing: 0.95; color: "#80FFFFFF" }
                    Row {
                        anchors.right: parent.right; anchors.rightMargin: 16; spacing: 4
                        Mono { id: bitrateValue; text: root.format(root.read("bitrateMbps"), 1); font.pixelSize: 12; color: root.metricColor }
                        Mono { anchors.baseline: bitrateValue.baseline; text: "/ " + (root.allocatedBitrateMbps > 0 ? root.format(root.allocatedBitrateMbps) : root.format(null)) + " Mbps"; font.pixelSize: 10; font.weight: Font.Medium; color: "#73FFFFFF" }
                    }
                    Rectangle {
                        objectName: "statsBitrateTrack"
                        x: 16; y: 23; width: parent.width - 32; height: 4; radius: 2; color: "#1AFFFFFF"
                        Rectangle { objectName: "statsBitrateFill"; width: parent.width * root.bitrateUsage; height: parent.height; radius: 2; color: root.accent }
                        Accessible.role: Accessible.ProgressBar
                        Accessible.name: qsTr("Allocated bitrate usage")
                        Accessible.description: root.format(root.read("bitrateMbps"), 1) + " / " + root.format(root.allocatedBitrateMbps) + " Mbps"
                    }
                }
                Repeater {
                    model: root.ledgerCards
                    delegate: Item {
                        id: ledger
                        required property var modelData
                        width: panelContents.width; height: 33
                        Rule { width: parent.width }
                        RowLayout {
                            anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 10
                            Mono { text: ledger.modelData.field === "videoDropCount" ? qsTr("FRAME DROPS") : ledger.modelData.label; font.pixelSize: 10; font.letterSpacing: 0.8; color: "#8CFFFFFF" }
                            Mono { Layout.fillWidth: true; text: root.ledgerDetail(ledger.modelData); font.pixelSize: 10; color: "#73FFFFFF"; font.weight: Font.Medium }
                            Mono {
                                text: root.format(ledger.modelData.value, ledger.modelData.decimals)
                                    + (ledger.modelData.field === "videoDropCount" ? "" : ledger.modelData.unit === "%" ? "%" : " " + ledger.modelData.unit)
                                color: root.degraded && ledger.modelData.key === "PacketLoss" ? "#D15A2C"
                                    : ledger.modelData.key === "Decode" ? "white" : root.metricColor
                            }
                        }
                    }
                }
                Item {
                    visible: root.frameGenerationEnabled
                    width: parent.width; height: 33
                    Rule { width: parent.width }
                    Mono { objectName: "expandedFrameGenerationState"; x: 16; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 32; text: qsTr("FRAME GENERATION") + " · " + root.frameGenerationState(); font.pixelSize: 10; color: "#8CFFFFFF" }
                }
                Item {
                    width: parent.width; height: Math.max(41, footer.implicitHeight + 22)
                    Rule { width: parent.width }
                    RowLayout {
                        id: footer
                        anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; anchors.topMargin: 10; anchors.bottomMargin: 12
                        spacing: 10
                        Mono { Layout.fillWidth: true; text: root.shown("Video") ? root.videoText : ""; font.pixelSize: 10; color: "#8CFFFFFF"; font.weight: Font.Medium; wrapMode: Text.Wrap }
                        Item {
                            implicitWidth: cycleKey.implicitWidth; implicitHeight: 18
                            KeyboardGlyph { id: cycleKey; shortcut: root.toggleShortcut; keySize: 18; ink: "white" }
                            TapHandler { onTapped: root.cycleRequested() }
                            Accessible.role: Accessible.Button; Accessible.name: qsTr("Hide stream statistics"); Accessible.onPressAction: root.cycleRequested()
                        }
                        Item {
                            implicitWidth: copyKey.implicitWidth; implicitHeight: 18
                            KeyboardGlyph { id: copyKey; shortcut: "Shift+F3"; keySize: 18; ink: "white" }
                            TapHandler { onTapped: root.copyRequested() }
                            Accessible.role: Accessible.Button; Accessible.name: qsTr("Copy stream statistics"); Accessible.onPressAction: root.copyRequested()
                        }
                    }
                }
            }
        }
    }
    Rectangle {
        id: clockPill
        visible: !root.expanded && root.shown("Clock")
        width: (clock.implicitWidth + 36) * root.overlayScale; height: 40 * root.overlayScale; radius: height / 2
        x: root.rightAligned ? root.inset : root.width - width - root.inset
        y: (root.bottomAligned ? root.height - height - root.inset : root.inset)
            + (root.width < compact.width + width + root.inset * 3 ? (root.bottomAligned ? -compact.height - 8 : compact.height + 8) : 0)
        color: root.surface; border.color: "#24FFFFFF"
        Clock { id: clock; anchors.centerIn: parent; scale: root.overlayScale }
    }
}

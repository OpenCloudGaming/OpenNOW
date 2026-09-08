pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

// UI only; never takes gameplay focus or owns media transport.
Item {
    id: root
    objectName: "desktopStreamStats"
    property bool expanded: false
    property bool pointerLocked: false
    property var frameGenerationStats: ({})
    // A HUD must never compete with gameplay for hover, taps or wheel input.
    // Shortcut-driven cycling/copy remains available while pointer input is off.
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
    readonly property color surface: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b,
        Math.max(0.4, Math.min(1, Number(ShellStore.settings.statsOverlayOpacity || 85) / 100)))
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
    readonly property string videoText: {
        const parts = []
        if (live.codec || profile.codec) parts.push(String(live.codec || profile.codec).toUpperCase())
        const w = Number(profile.width || live.outputWidth || 0), h = Number(profile.height || live.outputHeight || 0)
        if (w && h) parts.push(w + "×" + h)
        if (profile.bitDepth) parts.push(profile.bitDepth + "-bit")
        if (profile.chroma) parts.push(String(profile.chroma))
        if (profile.hdr === true) parts.push("HDR")
        return parts.length ? parts.join(" · ") : qsTr("Video format unavailable")
    }
    function metricCards() {
        // A negotiated target is not measured FPS. Discovery ping is not
        // stream RTT. Unsupported measurements are explicitly unavailable.
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
    onTelemetryActiveChanged: resetHistory()
    Connections { target: ShellStore; function onStreamStartedAtMsChanged() { root.resetHistory() } }
    Timer {
        running: root.visible; repeat: true; interval: 1000
        onTriggered: {
            root.nowMs = Date.now()
            if (!root.expanded || !root.shown("Graphs") || !root.telemetryActive) return
            const next = ({})
            for (const card of root.cards) {
                const values = (root.history[card.field] || []).slice(-59)
                values.push(root.sample(card)); next[card.field] = values
            }
            root.history = next
        }
    }
    Rectangle {
        id: compact
        objectName: "compactStatsBar"
        visible: !root.expanded
        width: Math.min(root.width - root.inset * 2, Math.max(160,
            (root.compactMetrics.reduce((sum, item) => sum + item.text.length * 7.4 + 12, 0)
                + String(ShellStore.settings.shortcutToggleStats || "Ctrl+N").length * 7.4 + 62) * root.overlayScale + 24))
        height: compactFlow.height + 16
        x: root.rightAligned ? root.width - width - root.inset : root.inset
        y: root.bottomAligned ? root.height - height - root.inset : root.inset
        radius: Math.min(20, height / 2); color: root.surface
        border.width: 1; border.color: Theme.seam
        Flow {
            id: compactFlow
            x: 12; y: 8
            width: compact.width - 24
            spacing: 12 * root.overlayScale
            Repeater {
                model: root.compactMetrics
                delegate: Text {
                    required property var modelData
                    text: modelData.text; color: Theme.label; font.family: Theme.monoFont
                    font.pixelSize: 12 * root.overlayScale; font.weight: Font.DemiBold
                }
            }
            Text {
                text: String(ShellStore.settings.shortcutToggleStats || "Ctrl+N") + " · " + qsTr("more")
                color: Theme.focus; font.family: Theme.monoFont; font.pixelSize: 12 * root.overlayScale
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.cycleRequested() }
    }
    Rectangle {
        id: expandedPanel
        objectName: "expandedStatsPanel"
        visible: root.expanded
        width: Math.min(460 * root.overlayScale, root.width - root.inset * 2)
        height: Math.min(panelContents.implicitHeight + 20, root.height - root.inset * 2)
        x: root.rightAligned ? root.width - width - root.inset : root.inset
        y: root.bottomAligned ? root.height - height - root.inset : root.inset
        radius: 22; color: root.surface; border.width: 1; border.color: Theme.seam
        Flickable {
            interactive: !root.pointerLocked
            anchors.fill: parent; anchors.margins: 10
            contentWidth: width; contentHeight: panelContents.implicitHeight
            clip: true; boundsBehavior: Flickable.StopAtBounds
            ColumnLayout {
                id: panelContents
                width: parent.width; spacing: 8
                RowLayout {
                    Layout.fillWidth: true; Layout.margins: 6
                    Rectangle { implicitWidth: 8; implicitHeight: 8; radius: 4; color: root.telemetryActive ? DesktopTokens.green : DesktopTokens.ledAmber }
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 2
                        Text { text: root.telemetryActive ? qsTr("Stream statistics") : qsTr("Waiting for stream"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 14 * root.overlayScale; font.weight: Font.Bold }
                        Text { visible: root.shown("Region"); Layout.fillWidth: true; text: root.region + (root.rig ? " · " + root.rig : ""); elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 12 * root.overlayScale }
                        Text {
                            objectName: "expandedFrameGenerationState"
                            visible: root.frameGenerationEnabled; Layout.fillWidth: true
                            text: qsTr("FRAME GENERATION") + " · " + root.frameGenerationState()
                            wrapMode: Text.WordWrap; color: Theme.textMuted
                            font.family: Theme.bodyFont; font.pixelSize: 12 * root.overlayScale
                        }
                    }
                    Text { visible: root.shown("Clock"); text: root.elapsedText(); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12 * root.overlayScale }
                }
                GridLayout {
                    Layout.fillWidth: true; columns: width < 360 ? 1 : 2; columnSpacing: 6; rowSpacing: 6
                    Repeater {
                        model: root.cards
                        delegate: Rectangle {
                            id: card
                            required property var modelData
                            Layout.fillWidth: true; Layout.preferredHeight: 82 * root.overlayScale
                            color: DesktopTokens.raised; radius: 14
                            Text { x: 12; y: 10; width: parent.width - 24; text: card.modelData.label; elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11 * root.overlayScale; font.weight: Font.Bold; font.letterSpacing: 0.5 }
                            Row {
                                x: 12; anchors.bottom: parent.bottom; anchors.bottomMargin: 12; spacing: 4
                                Text { id: metricValue; text: root.format(card.modelData.value, card.modelData.decimals); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 22 * root.overlayScale; font.weight: Font.DemiBold }
                                Text { anchors.baseline: metricValue.baseline; text: card.modelData.unit; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11 * root.overlayScale }
                            }
                            Canvas {
                                id: graph
                                visible: root.shown("Graphs") && card.modelData.value !== null
                                width: parent.width * 0.32; height: 24
                                anchors.right: parent.right; anchors.rightMargin: 12
                                anchors.bottom: parent.bottom; anchors.bottomMargin: 12
                                readonly property var samples: root.history[card.modelData.field] || []
                                onSamplesChanged: requestPaint()
                                onWidthChanged: requestPaint()
                                Connections { target: Theme; function onFocusChanged() { graph.requestPaint() } }
                                onPaint: {
                                    const ctx = getContext("2d")
                                    ctx.clearRect(0, 0, width, height)
                                    const valid = samples.filter(v => v !== null)
                                    if (valid.length < 2) return
                                    const low = Math.min(...valid), high = Math.max(...valid)
                                    ctx.strokeStyle = Theme.focus; ctx.lineWidth = 1.5; ctx.beginPath()
                                    let started = false
                                    for (let i = 0; i < samples.length; ++i) {
                                        if (samples[i] === null) { started = false; continue }
                                        const x = i * width / Math.max(1, samples.length - 1)
                                        const y = high === low ? height / 2 : height - 3 - (samples[i] - low) / (high - low) * (height - 6)
                                        if (started) ctx.lineTo(x, y); else ctx.moveTo(x, y)
                                        started = true
                                    }
                                    ctx.stroke()
                                }
                            }
                        }
                    }
                }
                Rectangle {
                    visible: root.shown("Video"); Layout.fillWidth: true; implicitHeight: videoLabel.implicitHeight + 24
                    radius: 12; color: DesktopTokens.raised
                    Text { id: videoLabel; x: 12; y: 12; width: parent.width - 24; text: root.videoText; wrapMode: Text.WordWrap; color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12 * root.overlayScale }
                }
                Text {
                    Layout.fillWidth: true; Layout.margins: 6
                    text: String(ShellStore.settings.shortcutToggleStats || "Ctrl+N") + " · " + qsTr("bar / panel / off") + "     Shift+F3 · " + qsTr("copy")
                    wrapMode: Text.WordWrap; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 11 * root.overlayScale
                }
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.cycleRequested() }
    }
    Rectangle {
        id: clockPill
        visible: !root.expanded && root.shown("Clock")
        width: clock.implicitWidth + 28; height: clock.implicitHeight + 18; radius: height / 2
        x: root.rightAligned ? root.inset : root.width - width - root.inset
        y: (root.bottomAligned ? root.height - height - root.inset : root.inset)
            + (root.width < compact.width + width + root.inset * 3 ? (root.bottomAligned ? -compact.height - 8 : compact.height + 8) : 0)
        color: root.surface; border.width: 1; border.color: Theme.seam
        Text { id: clock; anchors.centerIn: parent; text: root.elapsedText(); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12 * root.overlayScale; font.weight: Font.DemiBold }
    }
}

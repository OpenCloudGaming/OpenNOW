pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import OpenNOW

FocusScope {
    id: root
    width: 1440
    height: 900
    focus: false
    property bool expanded: false
    property double nowMs: Date.now()
    property var fpsSamples: []
    property var bitrateSamples: []
    signal cycleRequested()
    signal copyRequested()
    signal closeRequested()

    readonly property var live: ShellStore.streamer || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile
        || session.streamProfile || ShellStore.runtimeStreamProfile || ({})
    readonly property bool liveTelemetryAvailable: ["starting", "negotiating", "connecting", "streaming", "error"]
        .indexOf(String(live.status || "")) >= 0

    function number(value) {
        return value === undefined || value === null || isNaN(Number(value))
            ? null : Number(value)
    }
    function first(primary, fallback) {
        return root.number(primary) === null ? fallback : primary
    }
    function metric(value, decimals) {
        const parsed = root.number(value)
        return parsed === null ? qsTr("N/A")
            : (decimals > 0 ? parsed.toFixed(decimals) : String(Math.round(parsed)))
    }
    function unit(value, suffix) {
        return value === qsTr("N/A") ? value : value + suffix
    }
    function elapsedText() {
        if (ShellStore.streamStartedAtMs <= 0)
            return "0:00:00"
        const total = Math.floor(Math.max(0, nowMs - ShellStore.streamStartedAtMs) / 1000)
        const hours = Math.floor(total / 3600)
        const minutes = Math.floor((total % 3600) / 60)
        const seconds = total % 60
        return hours + ":" + String(minutes).padStart(2, "0")
            + ":" + String(seconds).padStart(2, "0")
    }
    function appendSample(samples, value) {
        const parsed = root.number(value)
        if (parsed === null)
            return samples
        const next = samples.slice(Math.max(0, samples.length - 22))
        next.push(parsed)
        return next
    }
    function sampleAt(samples, index) {
        const offset = 24 - samples.length
        return index < offset ? null : samples[index - offset]
    }
    function sampleHeight(samples, index) {
        const value = root.sampleAt(samples, index)
        if (value === null)
            return 3
        let low = value
        let high = value
        for (let i = 0; i < samples.length; ++i) {
            low = Math.min(low, samples[i])
            high = Math.max(high, samples[i])
        }
        if (high <= low)
            return 13
        return 7 + Math.round((value - low) / (high - low) * 17)
    }
    function rangeText(samples, unitText) {
        if (!samples.length)
            return qsTr("waiting for samples")
        let low = samples[0]
        let high = samples[0]
        for (let i = 1; i < samples.length; ++i) {
            low = Math.min(low, samples[i])
            high = Math.max(high, samples[i])
        }
        return qsTr("min %1 · max %2 %3")
            .arg(low.toFixed(1)).arg(high.toFixed(1)).arg(unitText)
    }

    readonly property var streamFpsNumber: root.liveTelemetryAvailable
        ? root.first(live.framesPerSecond, live.fps) : null
    readonly property var bitrateNumber: root.liveTelemetryAvailable
        ? root.first(live.bitrateMbps, live.receiveBitrateMbps) : null
    readonly property string streamFps: root.metric(root.streamFpsNumber, 0)
    readonly property string gameFps: root.metric(root.first(live.gameFramesPerSecond,
        root.first(profile.fps, live.outputFps)), 0)
    readonly property string latency: root.metric(root.first(live.latencyMs, live.roundTripLatencyMs), 0)
    readonly property string ping: root.metric(root.first(live.pingMs, session.latencyMs), 0)
    readonly property string bitrate: root.metric(root.bitrateNumber, 0)
    readonly property string peakBitrate: root.metric(live.peakBitrateMbps, 0)
    readonly property string queueDrops: root.metric(root.first(live.queueDropCount, 0), 0)
    readonly property string region: String(session.regionName || session.region
        || session.serverRegionId || "GFN").toUpperCase()
    readonly property string rig: String(session.rigName || session.gpuName
        || session.gpuType || "NVIDIA").toUpperCase()
    readonly property string codec: String(live.codec || profile.codec
        || ShellStore.runtimeStreamProfile.codec || ShellStore.settings.codec
        || qsTr("Automatic")).toUpperCase()
    readonly property string transport: String(live.transport || "NVST").toUpperCase()
    readonly property string mediaBackend: String(live.mediaBackend || qsTr("Pending")).toUpperCase()
    readonly property int outputHeight: Number(profile.height || live.outputHeight || 0)
    readonly property int refreshRate: Number(profile.fps || profile.frameRate || live.outputFps || 0)
    readonly property bool hdr: Boolean(profile.hdr)
    readonly property string outputText: (root.outputHeight > 0
        ? root.outputHeight + "p" : qsTr("Pending"))
        + (root.refreshRate > 0 ? " · " + root.refreshRate + " Hz" : "")
        + (root.hdr ? " · HDR" : "")

    function metricRows() {
        return [
            { label: "FPS", main: root.unit(root.streamFps, " fps"), secondary: root.unit(root.gameFps, " game") },
            { label: qsTr("LATENCY"), main: root.unit(root.latency, " ms"), secondary: "" },
            { label: qsTr("PING"), main: root.unit(root.ping, " ms"), secondary: "" },
            { label: qsTr("BITRATE"), main: root.unit(root.bitrate, " Mbps"), secondary: root.unit(root.peakBitrate, " peak") },
            { label: qsTr("DROPS"), main: root.queueDrops, secondary: qsTr("queue") }
        ]
    }

    Rectangle {
        id: compactPill
        anchors.top: parent.top
        anchors.topMargin: 24
        anchors.right: parent.right
        anchors.rightMargin: 24
        width: compactRow.implicitWidth + 24
        height: 34
        radius: 17
        visible: !root.expanded
        color: "#EB0E1018"
        border.width: 1
        border.color: DesktopTokens.seam

        Row {
            id: compactRow
            anchors.centerIn: parent
            spacing: 9
            Rectangle { width: 6; height: 6; radius: 3; anchors.verticalCenter: parent.verticalCenter; color: DesktopTokens.green }
            Text { anchors.verticalCenter: parent.verticalCenter; text: root.unit(root.streamFps, " fps"); color: DesktopTokens.textHigh; font.family: DesktopTokens.monoFont; font.pixelSize: 10; font.weight: Font.Bold }
            Rectangle { width: 1; height: 14; anchors.verticalCenter: parent.verticalCenter; color: DesktopTokens.seam }
            Text { anchors.verticalCenter: parent.verticalCenter; text: root.transport; color: DesktopTokens.textMuted; font.family: DesktopTokens.monoFont; font.pixelSize: 10; font.weight: Font.Bold }
            Rectangle { width: 1; height: 14; anchors.verticalCenter: parent.verticalCenter; color: DesktopTokens.seam }
            Text { anchors.verticalCenter: parent.verticalCenter; text: root.codec; color: DesktopTokens.focus; font.family: DesktopTokens.monoFont; font.pixelSize: 10; font.weight: Font.Bold }
            Rectangle { width: 1; height: 14; anchors.verticalCenter: parent.verticalCenter; color: DesktopTokens.seam }
            Text { anchors.verticalCenter: parent.verticalCenter; text: root.unit(root.bitrate, " Mbps"); color: DesktopTokens.textHigh; font.family: DesktopTokens.monoFont; font.pixelSize: 10; font.weight: Font.Bold }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.cycleRequested() }
    }

    MultiEffect {
        anchors.fill: expandedPanel
        source: expandedPanel
        visible: false
        shadowEnabled: true
        shadowColor: "#CC000000"
        shadowOpacity: 0.8
        shadowBlur: 1
        shadowVerticalOffset: 12
    }

    Rectangle {
        id: expandedPanel
        anchors.top: parent.top
        anchors.topMargin: 24
        anchors.right: parent.right
        anchors.rightMargin: 24
        width: 292
        height: 358
        radius: 18
        visible: root.expanded
        color: "#F20E1018"
        border.width: 1
        border.color: DesktopTokens.seam

        Item {
            x: 13; y: 10; width: parent.width - 26; height: 16
            Rectangle { width: 6; height: 6; radius: 3; anchors.verticalCenter: parent.verticalCenter; color: DesktopTokens.green }
            Text { x: 13; width: 170; anchors.verticalCenter: parent.verticalCenter; text: root.region + " · " + root.rig; color: DesktopTokens.textHigh; font.family: DesktopTokens.monoFont; font.pixelSize: 9; font.weight: Font.Bold; font.letterSpacing: 0.4; elide: Text.ElideRight }
            Text { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: root.elapsedText(); color: DesktopTokens.textMuted; font.family: DesktopTokens.monoFont; font.pixelSize: 9; font.weight: Font.DemiBold }
        }
        Rectangle { x: 13; y: 34; width: parent.width - 26; height: 1; color: DesktopTokens.seam }

        Column {
            x: 13; y: 42; width: parent.width - 26; spacing: 0
            Repeater {
                model: root.metricRows()
                delegate: Item {
                    id: metricRow
                    required property var modelData
                    width: parent.width
                    height: 21
                    Text { width: 64; anchors.verticalCenter: parent.verticalCenter; text: metricRow.modelData.label; color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 8; font.weight: Font.Bold; font.letterSpacing: 0.6 }
                    Text { x: 66; width: 100; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: metricRow.modelData.main; color: DesktopTokens.textHigh; font.family: DesktopTokens.monoFont; font.pixelSize: 10; font.weight: Font.Bold; elide: Text.ElideRight }
                    Text { anchors.right: parent.right; width: 88; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: metricRow.modelData.secondary; color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 8; font.weight: Font.DemiBold; elide: Text.ElideRight }
                }
            }
        }

        Rectangle { x: 13; y: 151; width: parent.width - 26; height: 1; color: DesktopTokens.seam }
        Column {
            x: 13; y: 157; width: parent.width - 26; spacing: 0
            Repeater {
                model: [
                    { label: qsTr("CODEC"), main: root.codec, secondary: root.mediaBackend },
                    { label: qsTr("OUTPUT"), main: root.outputText, secondary: root.hdr ? "HDR" : "SDR" }
                ]
                delegate: Item {
                    id: videoRow
                    required property var modelData
                    width: parent.width; height: 21
                    Text { width: 64; anchors.verticalCenter: parent.verticalCenter; text: videoRow.modelData.label; color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 8; font.weight: Font.Bold; font.letterSpacing: 0.6 }
                    Text { x: 66; width: 132; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: videoRow.modelData.main; color: DesktopTokens.focus; font.family: DesktopTokens.monoFont; font.pixelSize: 10; font.weight: Font.Bold; elide: Text.ElideRight }
                    Text { anchors.right: parent.right; width: 62; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: videoRow.modelData.secondary; color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 8; font.weight: Font.Bold; elide: Text.ElideRight }
                }
            }
        }

        Rectangle { x: 13; y: 205; width: parent.width - 26; height: 1; color: DesktopTokens.seam }
        Item {
            x: 13; y: 212; width: parent.width - 26; height: 50
            Text { text: qsTr("FRAMETIME"); color: DesktopTokens.green; font.family: DesktopTokens.monoFont; font.pixelSize: 7; font.weight: Font.Bold; font.letterSpacing: 0.55 }
            Text { anchors.right: parent.right; text: root.rangeText(root.fpsSamples.map(value => 1000 / Math.max(1, value)), "ms"); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 7; font.weight: Font.Bold }
            Row {
                anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 24; spacing: 2
                Repeater {
                    model: 24
                    delegate: Rectangle {
                        required property int index
                        width: 9
                        height: root.sampleHeight(root.fpsSamples.map(value => 1000 / Math.max(1, value)), index)
                        anchors.bottom: parent.bottom
                        radius: 1
                        color: DesktopTokens.green
                        opacity: root.sampleAt(root.fpsSamples, index) === null ? 0.2 : 0.72
                    }
                }
            }
        }
        Item {
            x: 13; y: 268; width: parent.width - 26; height: 50
            Text { text: qsTr("BITRATE"); color: DesktopTokens.focus; font.family: DesktopTokens.monoFont; font.pixelSize: 7; font.weight: Font.Bold; font.letterSpacing: 0.55 }
            Text { anchors.right: parent.right; text: root.rangeText(root.bitrateSamples, "Mbps"); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 7; font.weight: Font.Bold }
            Row {
                anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 24; spacing: 2
                Repeater {
                    model: 24
                    delegate: Rectangle {
                        required property int index
                        width: 9
                        height: root.sampleHeight(root.bitrateSamples, index)
                        anchors.bottom: parent.bottom
                        radius: 1
                        color: DesktopTokens.focus
                        opacity: root.sampleAt(root.bitrateSamples, index) === null ? 0.2 : 0.72
                    }
                }
            }
        }

        Rectangle { x: 13; y: 328; width: parent.width - 26; height: 1; color: DesktopTokens.seam }
        Text { x: 13; y: 340; text: qsTr("F3 · PS · XB — CYCLE"); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 7; font.weight: Font.Bold; font.letterSpacing: 0.35 }
        Text { anchors.right: parent.right; anchors.rightMargin: 13; y: 340; text: qsTr("SHIFT+F3 COPY"); color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: 7; font.weight: Font.Bold; font.letterSpacing: 0.35 }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.cycleRequested() }
    }

    Timer {
        interval: 1000
        repeat: true
        running: root.visible
        onTriggered: {
            root.nowMs = Date.now()
            root.fpsSamples = root.appendSample(root.fpsSamples, root.streamFpsNumber)
            root.bitrateSamples = root.appendSample(root.bitrateSamples, root.bitrateNumber)
        }
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_F3 && (event.modifiers & Qt.ShiftModifier)) {
            root.copyRequested()
            event.accepted = true
        } else if (event.key === Qt.Key_F3) {
            root.cycleRequested()
            event.accepted = true
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.closeRequested()
            event.accepted = true
        }
    }
}

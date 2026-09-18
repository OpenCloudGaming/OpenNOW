import QtQuick
import OpenNOW

Column {
    id: page
    objectName: "desktopStatsSettings"
    required property real availableWidth
    required property var settingsScreen
    property bool metricsOpen: false

    width: page.availableWidth; spacing: DesktopTokens.px(20)
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("STATISTICS OVERLAY") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Show on stream launch")
            description: qsTr("Cycle compact bar, extended panel and off with your statistics shortcut")
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("showStatsOnLaunch",false); onValueChangedByUser: value => { page.settingsScreen.setSetting("showStatsOnLaunch",value); page.settingsScreen.setSetting("showNativeStreamerStats",value) } }
        }
        DesktopSettingsChoice {
            width: parent.width; glyph: "grid"; title: qsTr("Position")
            items: [{label:qsTr("Top left"),value:"top-left"},{label:qsTr("Top right"),value:"top-right"},{label:qsTr("Bottom left"),value:"bottom-left"},{label:qsTr("Bottom right"),value:"bottom-right"}]
            value: page.settingsScreen.valueSetting("statsOverlayPosition","top-right")
            onSelected: value => page.settingsScreen.setSetting("statsOverlayPosition",value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "arrows"; title: qsTr("Overlay scale")
            DesktopSettingsSlider { from: 0.85; to: 1.5; stepSize: 0.05; decimals: 2; suffix: "×"; value: Number(page.settingsScreen.valueSetting("statsOverlayScale",1)); onCommitted: value => page.settingsScreen.setSetting("statsOverlayScale",value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Background opacity")
            DesktopSettingsSlider { from: 40; to: 100; stepSize: 5; value: Number(page.settingsScreen.valueSetting("statsOverlayOpacity",85)); onCommitted: value => page.settingsScreen.setSetting("statsOverlayOpacity",Math.round(value)) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "clock"; title: qsTr("Standalone session timer")
            description: qsTr("Show a small timer while playing, independently of the statistics overlay.")
            showDivider: false
            DesktopSettingsToggle {
                checked: page.settingsScreen.boolSetting("sessionCounterEnabled",false)
                Accessible.name: qsTr("Standalone session timer")
                onValueChangedByUser: value => page.settingsScreen.setSetting("sessionCounterEnabled",value)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            objectName: "customizeStatsMetrics"
            width: parent.width; paperStyle: true; glyph: "sliders"; title: qsTr("Customize metrics")
            expandable: true; expanded: page.metricsOpen; showDivider: false
            onExpansionRequested: page.metricsOpen = !page.metricsOpen
        }
    }
    DesktopSettingsDisclosure {
        objectName: "statsMetricsDisclosure"
        width: parent.width; expanded: page.metricsOpen
        sourceComponent: DesktopSettingsPanel {
            width: page.availableWidth; paperStyle: true
            Repeater {
                model: [
                    {title:qsTr("PERFORMANCE"),metrics:[
                        {key:"statsShowFps",label:qsTr("Stream FPS"),glyph:"speed"},
                        {key:"statsShowDrops",label:qsTr("Frame drops"),glyph:"monitor"},
                        {key:"statsShowDecode",label:qsTr("Decode time"),glyph:"chip"},
                        {key:"statsShowLatency",label:qsTr("Latency"),glyph:"clock"},
                        {key:"statsShowGraphs",label:qsTr("Live graphs"),glyph:"wave"}
                    ]},
                    {title:qsTr("NETWORK"),metrics:[
                        {key:"statsShowPing",label:qsTr("Ping"),glyph:"wave"},
                        {key:"statsShowBitrate",label:qsTr("Bitrate"),glyph:"wave"},
                        {key:"statsShowJitter",label:qsTr("Jitter"),glyph:"wave"},
                        {key:"statsShowPacketLoss",label:qsTr("Packet loss"),glyph:"arrows"}
                    ]},
                    {title:qsTr("SESSION"),metrics:[
                        {key:"statsShowRegion",label:qsTr("Stream region and rig"),glyph:"globe"},
                        {key:"statsShowVideo",label:qsTr("Codec and video format"),glyph:"image"},
                        {key:"statsShowClock",label:qsTr("Timer in statistics overlay"),glyph:"clock"}
                    ]}
                ]
                delegate: Column {
                    id: metricGroup
                    required property var modelData
                    width: parent.width
                    DesktopSettingsSection { text: metricGroup.modelData.title }
                    Repeater {
                        model: metricGroup.modelData.metrics
                        delegate: DesktopSettingsRow {
                            id: metricRow
                            required property var modelData
                            required property int index
                            width: parent.width; paperStyle: true; glyph: modelData.glyph; title: modelData.label
                            showDivider: index < metricGroup.modelData.metrics.length - 1
                            DesktopSettingsToggle {
                                objectName: "renew-" + metricRow.modelData.key
                                checked: page.settingsScreen.boolSetting(metricRow.modelData.key,true)
                                Accessible.name: metricRow.modelData.label
                                onValueChangedByUser: value => page.settingsScreen.setSetting(metricRow.modelData.key,value)
                            }
                        }
                    }
                }
            }
        }
    }
}

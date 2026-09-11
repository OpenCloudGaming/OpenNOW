import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; spacing: 20
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("FRAME-RATE VIEW") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Show on stream launch")
            description: qsTr("Cycle compact bar, extended panel and off with your statistics shortcut")
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("showStatsOnLaunch",false); onValueChangedByUser: value => { page.settingsScreen.setSetting("showStatsOnLaunch",value); page.settingsScreen.setSetting("showNativeStreamerStats",value) } }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "grid"; title: qsTr("Position")
            DesktopSettingsSegmented {
                options: [{label:qsTr("Top left"),value:"top-left"},{label:qsTr("Top right"),value:"top-right"},{label:qsTr("Bottom left"),value:"bottom-left"},{label:qsTr("Bottom right"),value:"bottom-right"}]
                optionWidth: 102; selectedIndex: options.findIndex(item => item.value === page.settingsScreen.valueSetting("statsOverlayPosition","top-right"))
                onSelected: (index,item) => page.settingsScreen.setSetting("statsOverlayPosition",item.value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "arrows"; title: qsTr("Overlay scale")
            DesktopSettingsSlider { from: 0.85; to: 1.5; stepSize: 0.05; decimals: 2; suffix: "×"; value: Number(page.settingsScreen.valueSetting("statsOverlayScale",1)); onCommitted: value => page.settingsScreen.setSetting("statsOverlayScale",value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Background opacity"); showDivider: false
            DesktopSettingsSlider { from: 40; to: 100; stepSize: 5; value: Number(page.settingsScreen.valueSetting("statsOverlayOpacity",85)); onCommitted: value => page.settingsScreen.setSetting("statsOverlayOpacity",Math.round(value)) }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("VISIBLE METRICS") }
        GridLayout {
            width: parent.width; columns: width >= DesktopTokens.px(820) ? 2 : 1; columnSpacing: 0; rowSpacing: 0
            Repeater {
                model: [
                    {key:"statsShowFps",label:qsTr("Stream FPS"),glyph:"speed"},
                    {key:"statsShowRegion",label:qsTr("Stream region and rig"),glyph:"globe"},
                    {key:"statsShowPing",label:qsTr("Ping"),glyph:"wave"},
                    {key:"statsShowBitrate",label:qsTr("Bitrate"),glyph:"wave"},
                    {key:"statsShowJitter",label:qsTr("Jitter"),glyph:"wave"},
                    {key:"statsShowDrops",label:qsTr("Frame drops"),glyph:"monitor"},
                    {key:"statsShowPacketLoss",label:qsTr("Packet loss"),glyph:"arrows"},
                    {key:"statsShowDecode",label:qsTr("Decode time"),glyph:"chip"},
                    {key:"statsShowLatency",label:qsTr("Latency"),glyph:"clock"},
                    {key:"statsShowVideo",label:qsTr("Codec and video format"),glyph:"image"},
                    {key:"statsShowClock",label:qsTr("Session clock"),glyph:"clock"},
                    {key:"statsShowGraphs",label:qsTr("Live graphs"),glyph:"wave"}
                ]
                delegate: DesktopSettingsRow {
                    required property var modelData
                    Layout.fillWidth: true; Layout.preferredWidth: 1; paperStyle: true; glyph: modelData.glyph; title: modelData.label
                    DesktopSettingsToggle { objectName: "renew-" + modelData.key; checked: page.settingsScreen.boolSetting(modelData.key,true); onValueChangedByUser: value => page.settingsScreen.setSetting(modelData.key,value) }
                }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "bolt"; title: qsTr("L4S")
            description: qsTr("Request scalable low-latency transport for the next session"); showDivider: false
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("enableL4S",false); onValueChangedByUser: value => page.settingsScreen.setSetting("enableL4S",value) }
        }
    }
}

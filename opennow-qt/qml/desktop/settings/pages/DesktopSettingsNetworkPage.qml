import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; spacing: 20
    Component.onCompleted: ShellStore.refreshRegions()
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("CONNECTION") }
        DesktopSettingsChoice {
            objectName: "renewNetworkRegion"
            width: parent.width; glyph: "globe"; title: qsTr("Server region")
            description: ShellStore.regions.length ? qsTr("%1 streaming regions from your account").arg(ShellStore.regions.length) : qsTr("Sign in to discover available regions")
            maximumColumns: 4
            maximumOptionsHeight: DesktopTokens.px(420)
            filterPlaceholder: qsTr("Search regions…")
            items: page.settingsScreen.regionChoiceItems()
            value: {
                const selected = String(page.settingsScreen.valueSetting("region",""))
                const region = ShellStore.regions.find(item => item.url === selected || item.name === selected)
                return region ? region.url : selected
            }
            valueLabel: page.settingsScreen.currentRegionLabel()
            onSelected: value => page.settingsScreen.setSetting("region",value)
        }
        DesktopSettingsRow {
            objectName: "renewRegionLatency"
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Region latency")
            showDivider: false
            description: ShellStore.regionPingMessage || qsTr("Measure available regions before your next session")
            value: ShellStore.regionPingBusy || page.settingsScreen.currentRegionPing() === null ? ""
                : page.settingsScreen.valueSetting("region", "") === "" ? qsTr("Best: %1 ms").arg(page.settingsScreen.currentRegionPing())
                : page.settingsScreen.currentRegionPing() + " ms"
            DesktopSettingsButton {
                objectName: "renewRegionPingButton"
                text: ShellStore.regionPingPending ? qsTr("Loading…") : ShellStore.regionPingBusy ? qsTr("Pinging…") : qsTr("Ping regions")
                enabled: !ShellStore.regionPingBusy
                onClicked: ShellStore.pingRegions()
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("API PROXY") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "globe"; title: qsTr("Use proxy")
            description: qsTr("Applies to API calls only · the stream always goes direct")
            DesktopSettingsToggle { objectName: "renewProxyEnabled"; checked: page.settingsScreen.boolSetting("sessionProxyEnabled",false); onValueChangedByUser: value => page.settingsScreen.setSetting("sessionProxyEnabled",value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "arrows"; title: qsTr("Proxy address")
            description: qsTr("Leave empty to use a direct connection"); showDivider: false
            DesktopSettingsField {
                objectName: "renewProxyAddress"
                width: DesktopTokens.px(320)
                text: String(page.settingsScreen.valueSetting("sessionProxyUrl",""))
                placeholderText: qsTr("http://proxy.example:8080")
                Accessible.name: qsTr("Proxy address")
                onEditingFinished: page.settingsScreen.setSetting("sessionProxyUrl",text)
            }
        }
    }
    DesktopSettingsAdvanced {
        detail: qsTr("Low-latency transport")
        expanded: page.settingsScreen.advancedOpen; onClicked: page.settingsScreen.advancedOpen = !page.settingsScreen.advancedOpen
    }
    DesktopSettingsDisclosure {
        width: parent.width; expanded: page.settingsScreen.advancedOpen
        sourceComponent: DesktopSettingsPanel {
            width: page.availableWidth; paperStyle: true
            DesktopSettingsSection { text: qsTr("TRANSPORT") }
            DesktopSettingsRow {
                width: parent.width; paperStyle: true; glyph: "bolt"; title: qsTr("L4S")
                description: qsTr("Request scalable low-latency transport for the next session"); showDivider: false
                DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("enableL4S",false); onValueChangedByUser: value => page.settingsScreen.setSetting("enableL4S",value) }
            }
        }
    }
}

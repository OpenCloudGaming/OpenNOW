import QtQuick
import QtQuick.Controls
import OpenNOW

// C9 Co-op settings (route settings-coop), artboard 5UT-0. Local preferences live in
// CoopState.preferences; the upload row reads the host's live fan-out statistics.
Column {
    id: page
    objectName: "settingsCoop"
    required property real availableWidth
    required property var settingsScreen
    readonly property var coop: ShellStore.coop
    readonly property var preferences: coop.preferences
    readonly property int maxGuests: Number(preferences.maxGuests || 3)
    readonly property var presets: [
        {label: qsTr("Same as my stream settings"), value: "same"},
        {label: qsTr("720p60 · H.264 · 8–10 Mbps"), value: "720p60-h264"},
        {label: qsTr("1080p60 · H.265 · 12–15 Mbps"), value: "1080p60-h265"},
        {label: qsTr("1080p60 · AV1 · 12–15 Mbps"), value: "1080p60-av1"},
        {label: qsTr("1440p60 · AV1 · 20–25 Mbps"), value: "1440p60-av1"}
    ]
    readonly property var stats: coop.isHost ? coop.hostStats : null
    readonly property int guestCount: coop.guests.length
    readonly property real usedMbps: stats ? Number(stats.uploadKbps || 0) / 1000 : 0
    readonly property real neededMbps: coop.uploadNeededKbps / 1000
    readonly property real capacityMbps: coop.uploadCapacityKbps / 1000

    function mbps(value) {
        return Math.round(value * 10) / 10
    }

    width: page.availableWidth
    spacing: DesktopTokens.px(14)

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("CO-OP") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "person"
            title: qsTr("Default max guests")
            description: page.maxGuests === 1 ? qsTr("Your guest takes slot P2.")
                : page.maxGuests === 2 ? qsTr("Guests take slots P2 and P3.") : qsTr("Guests take slots P2 to P4.")
            DesktopSettingsSegmented {
                objectName: "settingsCoopMaxGuests"
                options: [{label: "1", value: 1, width: 38}, {label: "2", value: 2, width: 38}, {label: "3", value: 3, width: 38}]
                selectedIndex: page.maxGuests - 1
                Accessible.name: qsTr("Default max guests")
                onSelected: (index, value) => page.coop.setPreference("maxGuests", value.value)
            }
        }
        DesktopSettingsChoice {
            objectName: "settingsCoopPreset"
            width: parent.width; glyph: "monitor"
            title: qsTr("Co-op preset")
            description: qsTr("The video format guests get. Every guest's device must be able to play it.")
            items: page.presets
            value: page.preferences.preset
            onSelected: value => page.coop.setPreference("preset", value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "wave"
            title: qsTr("Match bitrate to guest count")
            description: qsTr("Starts co-op at 15 Mbps for 2 guests or 12 Mbps for 3. Bitrate is fixed once the stream runs.")
            DesktopSettingsToggle {
                objectName: "settingsCoopMatchBitrate"
                checked: Boolean(page.preferences.matchBitrate)
                Accessible.name: qsTr("Match bitrate to guest count")
                onValueChangedByUser: value => page.coop.setPreference("matchBitrate", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "globe"
            title: qsTr("Allow relay fallback")
            description: qsTr("Used only when a direct link is blocked. Adds a little delay.")
            DesktopSettingsToggle {
                objectName: "settingsCoopAllowRelay"
                checked: Boolean(page.preferences.allowRelay)
                Accessible.name: qsTr("Allow relay fallback")
                onValueChangedByUser: value => page.coop.setPreference("allowRelay", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "lock"
            title: qsTr("Require host approval")
            description: qsTr("Off: anyone with the code joins straight in.")
            DesktopSettingsToggle {
                objectName: "settingsCoopRequireApproval"
                checked: Boolean(page.preferences.requireApproval)
                Accessible.name: qsTr("Require host approval")
                onValueChangedByUser: value => page.coop.setPreference("requireApproval", value)
            }
        }
        DesktopSettingsRow {
            objectName: "settingsCoopStreamMenuRow"
            width: parent.width; paperStyle: true; glyph: "monitor"
            title: qsTr("Show Friends & co-op in the stream menu")
            description: qsTr("Create co-op and invite friends from Ctrl+G. A running co-op session always stays in the menu.")
            DesktopSettingsToggle {
                objectName: "settingsCoopStreamMenu"
                checked: page.preferences.showInStreamMenu !== false
                Accessible.name: qsTr("Show Friends & co-op in the stream menu")
                onValueChangedByUser: value => page.coop.setPreference("showInStreamMenu", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "clock"
            title: qsTr("Show guest latency")
            description: qsTr("Ping per guest in the drawer and lobby.")
            DesktopSettingsToggle {
                objectName: "settingsCoopShowLatency"
                checked: Boolean(page.preferences.showLatency)
                Accessible.name: qsTr("Show guest latency")
                onValueChangedByUser: value => page.coop.setPreference("showLatency", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "person"; showDivider: false
            title: qsTr("Guest display name")
            description: qsTr("Shown to hosts when you join.")
            DesktopSettingsField {
                objectName: "settingsCoopGuestName"
                width: DesktopTokens.settingsControlWidth
                text: String(page.preferences.guestDisplayName || "")
                placeholderText: ShellStore.friends.linked ? ShellStore.friends.displayName(ShellStore.friends.user)
                    : qsTr("Name for co-op")
                maximumLength: 32
                font.pixelSize: DesktopTokens.px(14)
                font.weight: Font.Bold
                Accessible.name: qsTr("Guest display name")
                onEditingFinished: {
                    const name = text.trim()
                    if (name !== String(page.preferences.guestDisplayName || ""))
                        page.coop.setPreference("guestDisplayName", name)
                }
            }
        }
    }

    DesktopSettingsPanel {
        objectName: "settingsCoopUpload"
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("UPLOAD") }
        Item {
            width: parent.width
            height: page.stats ? meter.height + DesktopTokens.px(26) : idle.implicitHeight + DesktopTokens.px(26)
            Text {
                id: idle
                visible: !page.stats
                x: DesktopTokens.settingsInset; y: DesktopTokens.px(8)
                width: parent.width - x * 2
                text: qsTr("Each guest needs about one stream bitrate of your upload. OpenNOW shows what co-op uses while you host.")
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12)
                lineHeightMode: Text.FixedHeight; lineHeight: DesktopTokens.px(17)
                wrapMode: Text.WordWrap
            }
            Column {
                id: meter
                visible: page.stats !== null
                x: DesktopTokens.settingsInset; y: DesktopTokens.px(8)
                width: Math.min(DesktopTokens.px(520), parent.width - x * 2)
                spacing: DesktopTokens.px(8)
                readonly property real scaleMbps: Math.max(page.capacityMbps, page.neededMbps, page.usedMbps, 1) * 1.083
                Item {
                    width: parent.width
                    height: DesktopTokens.px(16)
                    Text {
                        y: amount.y + amount.baselineOffset - baselineOffset
                        text: qsTr("IN USE · %1 GUESTS AT %2 MBPS").arg(page.guestCount)
                            .arg(page.mbps(Number(page.stats ? page.stats.streamKbps : 0) / 1000))
                        color: Theme.textMuted
                        font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10)
                        font.weight: Font.DemiBold; font.letterSpacing: 0.8
                    }
                    Row {
                        anchors.right: parent.right
                        spacing: DesktopTokens.px(4)
                        Text {
                            id: amount
                            text: qsTr("%1 Mbps").arg(page.mbps(page.usedMbps))
                            color: Theme.label
                            font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(13); font.weight: Font.Bold
                        }
                        Text {
                            visible: page.capacityMbps > 0
                            anchors.baseline: amount.baseline
                            text: qsTr("/ %1 Mbps").arg(page.mbps(page.capacityMbps))
                            color: DesktopTokens.textFaint
                            font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10); font.weight: Font.DemiBold
                        }
                    }
                }
                Rectangle {
                    width: parent.width
                    height: DesktopTokens.px(6)
                    radius: height / 2
                    color: DesktopTokens.raised
                    Rectangle {
                        width: parent.width * Math.min(1, page.usedMbps / meter.scaleMbps)
                        height: parent.height
                        radius: height / 2
                        color: page.coop.uploadShort ? DesktopTokens.ledAmber : DesktopTokens.flightMint
                    }
                    Rectangle {
                        visible: page.capacityMbps > 0
                        x: parent.width * Math.min(1, page.capacityMbps / meter.scaleMbps) - width / 2
                        y: -DesktopTokens.px(4)
                        width: 2; height: DesktopTokens.px(14); radius: 1
                        color: Theme.label
                    }
                }
                Text {
                    width: parent.width
                    text: page.coop.uploadShort
                        ? qsTr("Your upload can’t carry every guest at %1 Mbps. Lower the bitrate or the guest count.").arg(page.mbps(Number(page.stats.streamKbps || 0) / 1000))
                        : qsTr("Co-op needs about %1 Mbps of upload for this lobby.").arg(page.mbps(page.neededMbps))
                    color: page.coop.uploadShort ? DesktopTokens.flightWarning : Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12)
                    lineHeightMode: Text.FixedHeight; lineHeight: DesktopTokens.px(17)
                    wrapMode: Text.WordWrap
                }
            }
        }
    }
}

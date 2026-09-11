import QtQuick
import QtQuick.Controls
import OpenNOW

// The same inline disclosure/tile pattern as the resolution picker, for
// account-provided regions and longer lists such as interface languages.
Item {
    id: root
    property string title: ""
    property string description: ""
    property string glyph: "globe"
    property var items: []
    property var value: ""
    property string valueLabel: ""
    property bool expanded: false
    property bool showDivider: true
    property int maximumColumns: 0
    property real maximumOptionsHeight: DesktopTokens.px(260)
    property string filterPlaceholder: qsTr("Filter options…")
    signal selected(var value)
    readonly property var current: items.find(item => item.kind !== "heading" && String(item.value) === String(root.value))
    readonly property var groups: {
        const groups = [{label: "", items: []}]
        const query = search.text.trim().toLocaleLowerCase()
        for (const item of items) {
            if (item.kind === "heading") groups.push({label: item.label, items: []})
            else if (!query || String(item.label).toLocaleLowerCase().indexOf(query) >= 0
                     || groups[groups.length-1].label.toLocaleLowerCase().indexOf(query) >= 0)
                groups[groups.length-1].items.push(item)
        }
        return groups.filter(group => group.items.length)
    }
    readonly property real revealProgress: reveal.progress
    readonly property real optionsHeight: Math.min(maximumOptionsHeight, grid.implicitHeight + 12)
    implicitHeight: header.height + (search.height + optionsHeight + 28) * reveal.progress
    clip: true
    MotionProgress { id: reveal; shown: root.expanded; enterDuration: 200; exitDuration: 160 }
    onExpandedChanged: {
        if (expanded) { search.clear(); search.forceActiveFocus() }
        else selector.forceActiveFocus()
    }
    Rectangle {
        visible: reveal.present
        opacity: reveal.progress
        x: 8; width: parent.width-16; height: parent.height
        radius: 16; color: DesktopTokens.raised
        border.width: 1; border.color: Theme.focus
    }
    DesktopSettingsRow {
        id: header
        width: parent.width; paperStyle: true; glyph: root.glyph
        title: root.title; description: root.description
        expanded: root.expanded; expandable: true
        showDivider: root.showDivider && !root.expanded
        onExpansionRequested: root.expanded = !root.expanded
        DesktopSettingsButton {
            id: selector
            width: Math.min(DesktopTokens.px(260), root.width - DesktopTokens.px(110))
            menu: true
            text: root.valueLabel || (root.current ? root.current.label : String(root.value))
            Accessible.name: root.title + ": " + text
            onClicked: root.expanded = !root.expanded
        }
    }
    DesktopSettingsField {
        id: search
        visible: reveal.present
        enabled: root.expanded
        opacity: reveal.progress
        x: 20; y: header.height; width: parent.width - 40
        placeholderText: root.filterPlaceholder
        onTextChanged: scroll.contentY = 0
        Accessible.name: root.title + ": " + placeholderText
        Keys.onEscapePressed: event => { root.expanded = false; event.accepted = true }
    }
    Flickable {
        id: scroll
        visible: reveal.present
        enabled: root.expanded
        opacity: reveal.progress
        x: 20; y: search.y + search.height + 12; width: parent.width-40
        height: root.optionsHeight
        contentWidth: width; contentHeight: grid.implicitHeight
        clip: true; boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: scroll.contentHeight > scroll.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
        Keys.onEscapePressed: event => { root.expanded = false; event.accepted = true }
        Column {
            id: grid
            width: parent.width - 12; spacing: 14
            Repeater {
                model: root.groups
                delegate: Column {
                    required property var modelData
                    width: grid.width; spacing: 8
                    Text {
                        visible: modelData.label !== ""
                        text: modelData.label; color: Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(11)
                        font.weight: Font.ExtraBold; font.letterSpacing: 1
                    }
                    Flow {
                        width: parent.width; spacing: 8
                        readonly property int columns: Math.max(1, Math.min(root.maximumColumns || 1000, Math.floor((width+8)/(DesktopTokens.px(200)+8))))
                        Repeater {
                            model: modelData.items
                            delegate: AbstractButton {
                                id: tile
                                required property var modelData
                                readonly property bool chosen: enabled && String(modelData.value) === String(root.value)
                                objectName: "settingsChoice-" + String(modelData.value)
                                width: Math.floor((parent.width-(parent.columns-1)*8)/parent.columns)
                                height: DesktopTokens.px(56)
                                enabled: !modelData.disabled; opacity: enabled ? 1 : 0.45
                                hoverEnabled: true
                                Accessible.name: String(modelData.label) + " " + String(modelData.detail || "")
                                onClicked: { root.expanded = false; root.selected(modelData.value) }
                                Keys.onEscapePressed: event => { root.expanded = false; event.accepted = true }
                                background: Rectangle {
                                    radius: 12
                                    color: tile.chosen ? Theme.focus : tile.hovered ? DesktopTokens.raisedStrong : DesktopTokens.raised
                                    border.width: tile.activeFocus ? 2 : 1
                                    border.color: tile.activeFocus ? Theme.focus : DesktopTokens.seamSoft
                                }
                                DesktopSettingsIcon {
                                    id: optionIcon
                                    visible: !!tile.modelData.glyph
                                    anchors.left: parent.left; anchors.leftMargin: 12
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 20; height: 20
                                    glyph: tile.modelData.glyph || "controller"
                                    ink: tile.chosen ? Theme.focusText : Theme.label
                                }
                                Column {
                                    anchors.verticalCenter: parent.verticalCenter
                                    x: optionIcon.visible ? 42 : 12
                                    width: parent.width - x - 12; spacing: 2
                                    Text { width: parent.width; text: tile.modelData.label; color: tile.chosen ? Theme.focusText : Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(13); font.weight: Font.ExtraBold; elide: Text.ElideRight }
                                    Text { visible: text !== ""; width: parent.width; text: tile.modelData.detail || ""; color: tile.chosen ? Theme.focusText : tile.modelData.detailColor || Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12); elide: Text.ElideRight }
                                }
                            }
                        }
                    }
                }
            }
            Text { visible: root.groups.length === 0; text: qsTr("No matching options"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(13) }
        }
    }
}

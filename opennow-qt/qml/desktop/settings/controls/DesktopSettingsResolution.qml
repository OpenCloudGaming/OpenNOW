import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

// Inline, aspect-ratio grouped tile picker from Desktop Renew. No popup menu.
Item {
    id: root
    objectName: "renewResolutionPicker"
    property var items: []
    property string value: "1920x1080"
    property bool expanded: false
    property bool fitsMonitor: false
    signal selected(string value)
    readonly property var choices: items.filter(item => item.kind !== "heading")
    readonly property var available: choices.filter(item => !item.disabled)
    readonly property var current: choices.find(item => item.value === value)
    readonly property var groups: {
        const list = []
        let group = null
        for (const item of items) {
            if (item.kind === "heading") { group = {label:item.label.split(" ")[0], items:[]}; list.push(group) }
            else if (group) {
                const size = item.value.split("x").map(Number)
                const screenWidth = Screen.width * Screen.devicePixelRatio
                const screenHeight = Screen.height * Screen.devicePixelRatio
                if (!fitsMonitor || (size[0] <= screenWidth && size[1] <= screenHeight))
                    group.items.push(item)
            }
        }
        return list.filter(group => group.items.length)
    }
    readonly property real revealProgress: reveal.progress
    readonly property real optionsHeight: Math.min(DesktopTokens.px(250), gridContents.implicitHeight + 18)
    implicitHeight: header.height + (optionsHeight + 12) * reveal.progress
    clip: true
    MotionProgress { id: reveal; shown: root.expanded; enterDuration: 200; exitDuration: 160 }
    onExpandedChanged: {
        if (expanded) gridFlick.forceActiveFocus()
        else stepper.focusSelector()
    }
    function step(direction) {
        if (!available.length) return
        const index = available.findIndex(item => item.value === value)
        root.selected(available[(Math.max(0,index) + direction + available.length) % available.length].value)
    }
    Rectangle {
        visible: reveal.present; opacity: reveal.progress
        x: 8; y: 0; width: parent.width-16; height: parent.height
        radius: 16; color: DesktopTokens.raised; border.width: 1; border.color: Theme.focus
    }
    DesktopSettingsRow {
        id: header
        width: parent.width; paperStyle: true; glyph: "monitor"; expanded: root.expanded
        rowHeight: DesktopTokens.px(68)
        title: qsTr("Resolution")
        description: root.expanded ? qsTr("%1 available · Esc closes").arg(root.available.length)
            : root.current ? root.current.detail : root.value.replace("x", "×")
        showDivider: !root.expanded; expandable: true
        onExpansionRequested: root.expanded = !root.expanded
        DesktopSettingsStepper {
            id: stepper
            visible: !root.expanded
            text: root.current ? root.current.label : root.value.replace("x","×")
            previousEnabled: root.available.length > 1; nextEnabled: previousEnabled
            onPrevious: root.step(-1); onNext: root.step(1)
            onOpenRequested: root.expanded = true
        }
        DesktopSettingsSegmented {
            visible: root.expanded
            options: [{label: qsTr("All"), width: DesktopTokens.px(46)},
                {label: qsTr("Fits monitor"), width: DesktopTokens.px(108)}]
            selectedIndex: root.fitsMonitor ? 1 : 0
            onSelected: index => root.fitsMonitor = index === 1
        }
    }
    Flickable {
        id: gridFlick
        visible: reveal.present
        enabled: root.expanded
        opacity: reveal.progress
        x: 20; y: header.height; width: parent.width-40
        height: Math.max(0, root.optionsHeight - 4)
        contentWidth: width; contentHeight: gridContents.implicitHeight+14
        clip: true; boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
        Keys.onEscapePressed: event => { root.expanded = false; event.accepted = true }
        Column {
            id: gridContents; width: parent.width; spacing: 14
            Repeater {
                model: root.groups
                delegate: Column {
                    required property var modelData
                    width: gridContents.width; spacing: 8
                    Row {
                        spacing: 8
                        Rectangle { width: 20; height: 11; y: 2; radius: 2; color: "transparent"; border.width: 1; border.color: Theme.textMuted }
                        Text { text: modelData.label; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.ExtraBold; font.letterSpacing: 1.1 }
                        Text { text: "· " + modelData.items.length; color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 11 }
                    }
                    Flow {
                        width: parent.width; spacing: 8
                        Repeater {
                            model: modelData.items
                            delegate: AbstractButton {
                                id: tile
                                required property var modelData
                                readonly property bool selected: modelData.value === root.value
                                width: DesktopTokens.px(118); height: DesktopTokens.px(52)
                                enabled: !modelData.disabled; opacity: enabled || selected ? 1 : 0.45
                                hoverEnabled: true
                                Accessible.name: modelData.label + " " + modelData.detail
                                onClicked: { root.selected(modelData.value); root.expanded = false }
                                Keys.onEscapePressed: event => { root.expanded = false; event.accepted = true }
                                background: Rectangle {
                                    radius: 12; color: tile.selected ? Theme.focus : tile.hovered ? DesktopTokens.raisedStrong : DesktopTokens.raised
                                    border.width: 1; border.color: tile.activeFocus ? Theme.focus : DesktopTokens.seamSoft
                                }
                                Column {
                                    anchors.centerIn: parent; spacing: 0
                                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: tile.modelData.label; color: tile.selected ? Theme.focusText : Theme.label; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.ExtraBold }
                                    Text { anchors.horizontalCenter: parent.horizontalCenter; text: tile.modelData.detail; color: tile.selected ? Qt.rgba(Theme.focusText.r,Theme.focusText.g,Theme.focusText.b,0.64) : Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10.5) }
                                }
                            }
                        }
                    }
                }
            }
            Text { visible: root.groups.length === 0; text: qsTr("No resolutions match this monitor"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 13 }
        }
    }
    Rectangle {
        anchors.left: gridFlick.left; anchors.right: gridFlick.right
        anchors.bottom: gridFlick.bottom; height: 40
        visible: reveal.present && gridFlick.contentY + gridFlick.height < gridFlick.contentHeight - 2
        enabled: root.expanded
        opacity: reveal.progress
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 1; color: Theme.shell }
        }
        AbstractButton {
            anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom
            width: moreLabel.implicitWidth + 28; height: 26
            onClicked: gridFlick.contentY = Math.min(gridFlick.contentHeight - gridFlick.height,
                gridFlick.contentY + gridFlick.height * 0.8)
            background: Rectangle { radius: 13; color: Theme.shell; border.width: 1; border.color: parent.activeFocus ? Theme.focus : DesktopTokens.seamSoft }
            Text { id: moreLabel; anchors.centerIn: parent; text: qsTr("Scroll for more"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Bold }
        }
    }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root
    objectName: "settingsSegmentedControl"
    property var options: []
    property int selectedIndex: 0
    property int optionWidth: 72
    // Values (or labels) that are visible but not selectable, e.g. frame
    // rates the current membership tier does not entitle. Accepts raw values
    // matching options entries, or {label,value} objects.
    property var disabledValues: []
    // Prefix for each chip's objectName; controls whose values can collide with another
    // control on the same page (plain numbers) pass their own.
    property string optionObjectNamePrefix: "settingsOption-"
    // Optional per-option hint shown as a tooltip, e.g. "Requires Ultimate".
    property var disabledHint: ""
    signal selected(int index, var value)

    readonly property real optionSpacing: DesktopTokens.px(4)
    readonly property real naturalWidth: options.reduce((total, option) => total + root.widthFor(option), DesktopTokens.px(8))
        + Math.max(0, options.length - 1) * optionSpacing
    readonly property real availableWidth: parent && parent.availableWidth !== undefined
        ? parent.availableWidth : naturalWidth
    readonly property bool uniformWidths: options.length > 0
        && options.every(option => root.widthFor(option) === root.widthFor(options[0]))
    readonly property int columns: {
        if (!options.length || !uniformWidths || naturalWidth <= width)
            return Math.max(1, options.length)
        const fitting = Math.max(1, Math.floor((width - DesktopTokens.px(8) + optionSpacing)
            / (root.widthFor(options[0]) + optionSpacing)))
        return Math.ceil(options.length / Math.ceil(options.length / fitting))
    }
    implicitWidth: Math.min(naturalWidth, Math.max(0, availableWidth))
    implicitHeight: optionFlow.implicitHeight + DesktopTokens.px(8)

    function optionValue(option) {
        return (typeof option === "object" && option !== null && option.value !== undefined)
            ? option.value : option
    }

    function widthFor(option) {
        return typeof option === "object" && option !== null && option.width !== undefined
            ? DesktopTokens.px(Math.max(1, Number(option.width))) : DesktopTokens.px(optionWidth)
    }

    function optionLabel(option) {
        if (typeof option === "object" && option !== null)
            return String(option.label !== undefined ? option.label : option.value)
        return String(option)
    }

    function focusSelectedOption() {
        const selected = optionsRepeater.itemAt(selectedIndex)
        if (selected && selected.enabled) {
            selected.forceActiveFocus()
            return
        }
        for (let i = 0; i < optionsRepeater.count; ++i) {
            const option = optionsRepeater.itemAt(i)
            if (option && option.enabled) {
                option.forceActiveFocus()
                return
            }
        }
    }

    function isDisabled(option) {
        const value = String(root.optionValue(option))
        for (let i = 0; i < root.disabledValues.length; ++i) {
            const disabled = root.disabledValues[i]
            if (String(disabled) === value
                    || String(root.optionValue(disabled)) === value)
                return true
        }
        if (typeof option === "object" && option !== null && option.enabled === false)
            return true
        return false
    }

    Rectangle { anchors.fill: parent; radius: Math.min(height / 2, DesktopTokens.px(20)); color: Theme.lightMode ? Qt.rgba(0,0,0,0.04) : Qt.rgba(0,0,0,0.35); border.width: 1; border.color: Theme.seam }
    Flow {
        id: optionFlow
        objectName: "settingsSegmentedOptions"
        x: DesktopTokens.px(4); y: DesktopTokens.px(4)
        width: Math.max(0, root.width - DesktopTokens.px(8))
        spacing: root.optionSpacing
        Repeater {
            id: optionsRepeater
            model: root.options
            delegate: AbstractButton {
                id: chip
                required property int index
                required property var modelData
                readonly property bool on: index === root.selectedIndex && !locked
                readonly property bool locked: root.isDisabled(modelData)
                objectName: root.optionObjectNamePrefix + String(root.optionValue(modelData))
                readonly property string label: root.optionLabel(modelData)
                width: root.uniformWidths
                    ? (optionFlow.width - (root.columns - 1) * optionFlow.spacing) / root.columns
                    : Math.min(root.widthFor(modelData), optionFlow.width)
                height: DesktopTokens.px(32)
                hoverEnabled: true
                enabled: !locked
                onClicked: root.selected(chip.index, chip.modelData)
                background: Rectangle {
                    radius: height / 2
                    color: chip.on ? Theme.focus : chip.hovered ? DesktopTokens.raised : "transparent"
                    border.width: chip.activeFocus ? 2 : 0
                    border.color: Theme.focus
                }
                opacity: chip.locked ? 0.45 : 1

                Text {
                    anchors.centerIn: parent
                    width: parent.width - DesktopTokens.px(8)
                    text: chip.label
                    color: chip.on ? Theme.focusText : DesktopTokens.textBody
                    font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.px(13)
                    font.weight: chip.on ? Font.ExtraBold : Font.Bold
                    horizontalAlignment: Text.AlignHCenter
                    elide: Text.ElideRight
                }
                ToolTip.visible: chip.locked && chip.hovered && root.disabledHint !== ""
                ToolTip.text: root.disabledHint
                ToolTip.delay: 400
            }
        }
    }
}

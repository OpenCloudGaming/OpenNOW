import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root
    property var options: []
    property int selectedIndex: 0
    property int optionWidth: 72
    // Values (or labels) that are visible but not selectable, e.g. frame
    // rates the current membership tier does not entitle. Accepts raw values
    // matching options entries, or {label,value} objects.
    property var disabledValues: []
    // Optional per-option hint shown as a tooltip, e.g. "Requires Ultimate".
    property var disabledHint: ""
    signal selected(int index, var value)

    implicitWidth: Math.max(0, options.length * optionWidth + Math.max(0, options.length - 1) * 6)
    implicitHeight: 36

    function optionValue(option) {
        return (typeof option === "object" && option !== null && option.value !== undefined)
            ? option.value : option
    }

    function optionLabel(option) {
        if (typeof option === "object" && option !== null)
            return String(option.label !== undefined ? option.label : option.value)
        return String(option)
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

    Row {
        spacing: 6
        Repeater {
            model: root.options
            delegate: Rectangle {
                id: chip
                required property int index
                required property var modelData
                readonly property bool on: index === root.selectedIndex
                readonly property bool locked: root.isDisabled(modelData)
                readonly property string label: root.optionLabel(modelData)
                width: root.optionWidth
                height: DesktopTokens.px(36)
                radius: DesktopTokens.px(10)
                color: chip.on ? "#F2FFFFFF" : (chip.locked ? "#08FFFFFF" : (chipHover.hovered ? "#1AFFFFFF" : "#0FFFFFFF"))
                border.width: chip.on ? 0 : 1
                border.color: chip.on ? "transparent" : (chip.locked ? "#14FFFFFF" : "#26FFFFFF")
                opacity: chip.locked && !chip.on ? 0.45 : 1

                Text {
                    anchors.centerIn: parent
                    width: parent.width - 8
                    text: chip.locked && !chip.on ? "🔒 " + chip.label : chip.label
                    color: chip.on ? "#0B0F1A" : DesktopTokens.textBody
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.monoSize
                    font.weight: Font.Bold
                    horizontalAlignment: Text.AlignHCenter
                    elide: Text.ElideRight
                }
                ToolTip.visible: chip.locked && chipHover.hovered && root.disabledHint !== ""
                ToolTip.text: root.disabledHint
                ToolTip.delay: 400
                HoverHandler { id: chipHover; cursorShape: chip.locked ? Qt.ForbiddenCursor : Qt.PointingHandCursor }
                TapHandler {
                    onTapped: {
                        if (!chip.locked)
                            root.selected(chip.index, chip.modelData)
                    }
                }
            }
        }
    }
}

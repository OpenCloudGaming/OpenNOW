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

    implicitWidth: options.reduce((total, option) => total + root.widthFor(option), 8)
    implicitHeight: DesktopTokens.px(40)

    function optionValue(option) {
        return (typeof option === "object" && option !== null && option.value !== undefined)
            ? option.value : option
    }

    function widthFor(option) {
        return typeof option === "object" && option !== null && option.width !== undefined
            ? Math.max(1, Number(option.width)) : DesktopTokens.px(optionWidth)
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

    Rectangle { anchors.fill: parent; radius: height / 2; color: Theme.lightMode ? Qt.rgba(0,0,0,0.04) : Qt.rgba(0,0,0,0.35); border.width: 1; border.color: Theme.seam }
    Row {
        x: 4; y: 4
        spacing: 0
        Repeater {
            model: root.options
            delegate: AbstractButton {
                id: chip
                required property int index
                required property var modelData
                readonly property bool on: index === root.selectedIndex && !locked
                readonly property bool locked: root.isDisabled(modelData)
                objectName: "settingsOption-" + String(root.optionValue(modelData))
                readonly property string label: root.optionLabel(modelData)
                width: root.widthFor(modelData)
                height: root.height - 8
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
                    width: parent.width - 8
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

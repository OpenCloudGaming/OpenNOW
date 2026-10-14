import QtQuick
import QtQuick.Controls
import OpenNOW

DesktopSettingsRow {
    id: root
    required property var modelData
    required property string domain
    required property string sourceId
    readonly property var store: ShellStore.sourceOwnerState
    readonly property string kind: String(modelData.control.kind)
    readonly property var choices: modelData.control.choices || []
    objectName: "desktopSourceSetting-" + domain + "-" + modelData.key
    paperStyle: true
    textFormat: Text.PlainText
    glyph: "sliders"
    title: String(modelData.label)

    function apply(value) {
        store.setSetting(sourceId, modelData.key, {kind: kind, value: value}, domain)
    }

    function numeric(delta) {
        const control = modelData.control
        const step = Number(control.step || 1)
        return Math.max(Number(control.min), Math.min(Number(control.max), Number(modelData.value.value) + delta * step))
    }

    function choiceIndex() {
        return choices.findIndex(choice => choice.value === modelData.value.value)
    }

    function step(delta) {
        if (kind === "choice")
            apply(choices[Math.max(0, Math.min(choices.length - 1, choiceIndex() + delta))].value)
        else
            apply(numeric(delta))
    }

    DesktopSettingsToggle {
        visible: root.kind === "boolean"
        checked: root.modelData.value.value === true
        Accessible.name: String(root.modelData.label)
        onValueChangedByUser: value => root.apply(value)
    }
    DesktopSettingsSegmented {
        visible: root.kind === "choice" && root.choices.length <= 4
        options: root.kind === "choice" ? root.choices : []
        optionWidth: 110
        selectedIndex: root.choiceIndex()
        onSelected: (index, item) => root.apply(item.value)
    }
    DesktopSettingsStepper {
        visible: root.kind === "integer" || root.kind === "number" || (root.kind === "choice" && root.choices.length > 4)
        text: root.kind === "choice" && root.choiceIndex() >= 0 ? String(root.choices[root.choiceIndex()].label)
            : String(root.modelData.value.value)
        onPrevious: root.step(-1)
        onNext: root.step(1)
    }
    DesktopSettingsField {
        visible: root.kind === "text"
        text: root.kind === "text" ? String(root.modelData.value.value) : ""
        maximumLength: Number(root.modelData.control.maximumBytes || 256)
        onAccepted: root.apply(text)
    }
}

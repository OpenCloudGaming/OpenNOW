import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

// Capability test results for every stream frame rate at once. The GPU is measured once
// per resolution (the cost per generated frame does not depend on the rate); every cell
// is decided from that measurement, the rate of its row and the display refresh. Picking
// a row chooses the stream frame rate, picking a cell the level; Apply writes the settings.
Dialog {
    id: root
    objectName: "frameGenerationTestDialog"
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(DesktopTokens.px(840), parent.width - DesktopTokens.px(32))
    contentWidth: width - leftPadding - rightPadding
    padding: DesktopTokens.px(22)
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape
    title: qsTr("Frame generation capability test")

    // Draft selection, written to settings by Apply.
    property int sourceFps: 60
    property string mode: "off"
    property int level: 2
    property int target: 0
    readonly property var fg: ShellStore.settingsOwnerState
    readonly property var profile: fg.frameGenerationProfile()
    readonly property bool measured: !!profile
    readonly property var matrix: ShellStore.frameGenerationMatrix()
    readonly property var levels: ["2x", "3x", "4x"]
    readonly property int displayHz: Math.round(ShellStore.frameGenerationDisplayHz())
    readonly property int detectedHz: Math.round(Number(fg.detectedDisplayHz || 0))
    readonly property int displayOverride: Number((ShellStore.settings || ({})).frameGenerationDisplayHz || 0)
    readonly property var lockedFps: ShellStore.lockedFpsValues(fg.frameGenerationResolution().text)
    readonly property int sourceColumn: DesktopTokens.px(118)
    readonly property int cellGap: DesktopTokens.px(8)
    readonly property int cellWidth: Math.floor((contentWidth - sourceColumn - cellGap * 3) / 3)

    function openFromSettings() {
        const settings = ShellStore.settings || ({})
        const fps = Number(settings.fps || 0)
        sourceFps = fps > 0 ? fps : ShellStore.frameGenerationSourceFps()
        mode = ShellStore.frameGenerationMode()
        level = Math.max(2, fg.frameGenerationLevelFactor(settings.frameGeneration))
        target = Number(settings.frameGenerationTargetFps || 0)
        open()
        if (!measured && !FrameGenerationBenchmark.running) ShellStore.runFrameGenerationTest()
    }
    function resolutionText() { return fg.frameGenerationResolution().text.replace("x", "×") }
    function reasonLabel(reason) {
        switch (String(reason || "")) {
        case "display": return qsTr("display")
        case "gpu": return qsTr("GPU")
        case "engine": return qsTr("engine")
        default: return qsTr("untested")
        }
    }
    function cellTip(cell) {
        switch (String(cell.reason || "")) {
        case "gpu": return qsTr("%1 ms needed, %2 ms budget").arg(cell.costMs.toFixed(1)).arg(cell.budgetMs.toFixed(1))
        case "display": return qsTr("The %1 Hz display cannot show %2 FPS").arg(root.displayHz).arg(cell.output)
        case "engine": return qsTr("This engine generates 2× only")
        default: return qsTr("Not measured yet.")
        }
    }
    function pick(fps, factor) {
        sourceFps = fps
        if (mode === "target") target = fps * factor
        else { mode = "multiplier"; level = factor }
    }
    function cellSelected(fps, factor) {
        if (fps !== sourceFps) return false
        if (mode === "multiplier") return level === factor
        if (mode === "target") return target === fps * factor
        return false
    }
    function statusText() {
        if (FrameGenerationBenchmark.running) return qsTr("Measuring this GPU at %1…").arg(resolutionText())
        if (fg.frameGenerationTestError !== "") return qsTr("Capability test failed: %1").arg(fg.frameGenerationTestError)
        if (!measured) return qsTr("Not measured yet.")
        return qsTr("%1 · %2 · %3 (p95 per source frame)")
            .arg(profile.gpu || fg.frameGenerationGpu).arg(resolutionText()).arg(fg.frameGenerationCostText(profile))
    }
    function apply() {
        const settings = ShellStore.settings || ({})
        if (Number(settings.fps || 0) !== sourceFps) ShellStore.setSetting("fps", sourceFps)
        if (mode === "multiplier") ShellStore.selectFrameGenerationLevel(levels[Math.min(4, Math.max(2, level)) - 2])
        else if (mode === "target") ShellStore.selectFrameGenerationTarget(target)
        else ShellStore.selectFrameGenerationMode(mode)
        close()
    }

    header: Text {
        width: root.width
        text: root.title
        padding: DesktopTokens.px(22)
        bottomPadding: 0
        color: Theme.label
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.px(20)
        font.weight: Font.ExtraBold
        wrapMode: Text.WordWrap
    }
    background: Rectangle { radius: DesktopTokens.px(16); color: Theme.shell; border.color: Theme.seam }

    component Hint: Text {
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.captionSize
        wrapMode: Text.WordWrap
    }
    component Caption: Text {
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.smallSize
        font.weight: Font.DemiBold
        font.capitalization: Font.AllUppercase
        width: root.sourceColumn
        verticalAlignment: Text.AlignVCenter
    }

    contentItem: ColumnLayout {
        spacing: DesktopTokens.px(12)
        Rectangle {
            objectName: "frameGenerationDialogNotice"
            Layout.fillWidth: true
            implicitHeight: noticeText.implicitHeight + DesktopTokens.px(16)
            radius: DesktopTokens.px(10)
            color: Qt.rgba(0.96, 0.65, 0.14, 0.12)
            border.color: Qt.rgba(0.96, 0.65, 0.14, 0.45)
            Text {
                id: noticeText
                anchors.fill: parent; anchors.margins: DesktopTokens.px(8)
                text: ShellStore.frameGenerationEngineNotice()
                color: "#F5A623"
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.captionSize
                font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
            }
        }
        Hint {
            objectName: "frameGenerationDialogStatus"
            Layout.fillWidth: true; text: root.statusText()
            color: root.measured && !FrameGenerationBenchmark.running ? Theme.label : Theme.textMuted
        }
        ProgressBar {
            Layout.fillWidth: true; visible: FrameGenerationBenchmark.running
            from: 0; to: 1; value: FrameGenerationBenchmark.progress
        }
        RowLayout {
            spacing: DesktopTokens.px(12)
            Caption { text: qsTr("Display") }
            DesktopSettingsSegmented {
                objectName: "frameGenerationDialogDisplay"
                optionObjectNamePrefix: "frameGenerationDialogDisplay-"
                options: [{label: root.detectedHz > 0 ? qsTr("Auto · %1 Hz").arg(root.detectedHz) : qsTr("Auto"), value: 0, width: 112}].concat(
                    ShellStore.frameGenerationDisplayChoices().map(hz => ({label: String(hz), value: hz})))
                optionWidth: 46
                selectedIndex: Math.max(0, options.findIndex(item => item.value === root.displayOverride))
                onSelected: (index, item) => ShellStore.selectFrameGenerationDisplayHz(item.value)
            }
        }
        RowLayout {
            spacing: DesktopTokens.px(12)
            Caption { text: qsTr("Stream FPS") }
            DesktopSettingsSegmented {
                objectName: "frameGenerationDialogSource"
                optionObjectNamePrefix: "frameGenerationDialogSource-"
                options: ShellStore.canonicalFpsValues().map(fps => ({label: String(fps), value: fps}))
                optionWidth: 48
                selectedIndex: options.findIndex(item => item.value === root.sourceFps)
                disabledValues: root.lockedFps
                disabledHint: ShellStore.lockedFpsReason() || qsTr("Not entitled on your current membership")
                onSelected: (index, item) => root.sourceFps = item.value
            }
        }
        RowLayout {
            spacing: DesktopTokens.px(12)
            Caption { text: qsTr("Mode") }
            DesktopSettingsSegmented {
                objectName: "frameGenerationDialogMode"
                optionObjectNamePrefix: "frameGenerationDialogMode-"
                options: [{label: qsTr("Off"), value: "off"}, {label: qsTr("Multiplier"), value: "multiplier"},
                          {label: qsTr("Target FPS"), value: "target"}, {label: qsTr("Display"), value: "display"}]
                optionWidth: 86
                selectedIndex: Math.max(0, options.findIndex(item => item.value === root.mode))
                onSelected: (index, item) => root.mode = item.value
            }
        }
        Hint {
            Layout.fillWidth: true
            text: root.mode === "display"
                ? qsTr("Display mode picks the largest level that fits the %1 Hz display by itself.").arg(root.displayHz)
                : qsTr("Pick a row for the stream frame rate and a cell for the level. Greyed cells say why they do not fit.")
        }
        Row {
            spacing: root.cellGap
            Caption { text: qsTr("Stream") }
            Repeater {
                model: [2, 3, 4]
                Text {
                    required property int modelData
                    width: root.cellWidth; horizontalAlignment: Text.AlignHCenter
                    text: modelData + "×"; color: Theme.textMuted; font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.smallSize; font.weight: Font.DemiBold
                }
            }
        }
        Column {
            Layout.fillWidth: true
            spacing: DesktopTokens.px(4)
            Repeater {
                model: root.matrix
                Rectangle {
                    id: rowItem
                    required property var modelData
                    readonly property bool current: modelData.fps === root.sourceFps
                    readonly property bool locked: modelData.locked === true
                    objectName: "frameGenerationDialogRow-" + modelData.fps
                    width: root.contentWidth; height: DesktopTokens.px(40)
                    radius: DesktopTokens.px(10)
                    color: current ? DesktopTokens.raisedStrong : "transparent"
                    opacity: locked ? 0.4 : 1
                    Row {
                        anchors.fill: parent; spacing: root.cellGap
                        AbstractButton {
                            width: root.sourceColumn; height: parent.height
                            enabled: !rowItem.locked
                            onClicked: root.sourceFps = rowItem.modelData.fps
                            contentItem: Text {
                                leftPadding: DesktopTokens.px(10)
                                verticalAlignment: Text.AlignVCenter
                                text: rowItem.modelData.fps + " FPS"; color: Theme.label; font.family: Theme.bodyFont
                                font.pixelSize: DesktopTokens.bodySize; font.weight: rowItem.current ? Font.Bold : Font.Normal
                            }
                            background: Item {}
                        }
                        Repeater {
                            model: rowItem.modelData.cells
                            AbstractButton {
                                id: cell
                                required property var modelData
                                readonly property bool allowed: modelData.allowed === true
                                readonly property bool pickable: allowed && !rowItem.locked && root.mode !== "display"
                                readonly property bool selected: root.cellSelected(rowItem.modelData.fps, modelData.factor)
                                objectName: "frameGenerationDialogCell-" + rowItem.modelData.fps + "-" + modelData.factor
                                width: root.cellWidth; height: parent.height
                                // Stays enabled so the tooltip explains a cell that does not fit.
                                hoverEnabled: true
                                ToolTip.visible: hovered && !allowed
                                ToolTip.delay: 300
                                ToolTip.text: root.cellTip(modelData)
                                onClicked: if (pickable) root.pick(rowItem.modelData.fps, modelData.factor)
                                background: Rectangle {
                                    radius: DesktopTokens.px(9)
                                    color: cell.selected ? Theme.focus : cell.hovered && cell.pickable ? DesktopTokens.raisedStrong : DesktopTokens.raised
                                    border.width: 1; border.color: cell.selected ? Theme.focus : Theme.seam
                                }
                                contentItem: Item {
                                    Row {
                                        anchors.centerIn: parent; spacing: DesktopTokens.px(6)
                                        Text {
                                            text: cell.modelData.output + " FPS"
                                            color: cell.selected ? Theme.focusText : cell.allowed ? Theme.label : Theme.textMuted
                                            font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                                            font.weight: cell.allowed ? Font.DemiBold : Font.Normal
                                        }
                                        Text {
                                            text: cell.allowed ? "✓" : root.reasonLabel(cell.modelData.reason)
                                            color: cell.selected ? Theme.focusText : cell.allowed ? DesktopTokens.mint : DesktopTokens.textFaint
                                            font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.smallSize
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    footer: DialogButtonBox {
        padding: DesktopTokens.px(16)
        implicitHeight: DesktopTokens.controlHeight + topPadding + bottomPadding
        background: Item {}
        DesktopSettingsButton {
            objectName: "frameGenerationDialogRerun"
            text: FrameGenerationBenchmark.running ? qsTr("Testing…") : qsTr("Run again")
            enabled: !FrameGenerationBenchmark.running
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: ShellStore.runFrameGenerationTest()
        }
        DesktopSettingsButton {
            objectName: "frameGenerationDialogCancel"
            text: qsTr("Cancel")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
        DesktopSettingsButton {
            objectName: "frameGenerationDialogApply"
            text: qsTr("Apply")
            primary: true
            enabled: root.measured && !FrameGenerationBenchmark.running
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            onClicked: root.apply()
        }
    }
}

pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import OpenNOW

Column {
    id: root
    required property var store
    readonly property var settings: store.onboardingSettings
    readonly property bool metalFx: settings.upscaling === "metalfx"
    readonly property bool mac: Qt.platform.os === "osx"
    readonly property bool wide: width >= DesktopTokens.px(1000)
    readonly property color mint: Theme.accentColor("green")
    readonly property color blue: Theme.accentColor("blue")
    readonly property color panelColor: Theme.lightMode ? Theme.glass : "#C70B0F1A"
    spacing: DesktopTokens.px(24)

    component Copy: Text {
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.px(13)
        wrapMode: Text.WordWrap
        lineHeightMode: Text.FixedHeight
        lineHeight: DesktopTokens.px(font.pixelSize >= DesktopTokens.px(20) ? 24 : font.pixelSize <= DesktopTokens.px(12) ? 16 : 18)
    }
    component Eyebrow: Text {
        color: Theme.textMuted
        font.family: Theme.monoFont
        font.pixelSize: DesktopTokens.px(10)
        font.weight: Font.Bold
        font.letterSpacing: DesktopTokens.px(1)
    }
    component DashedOutline: Shape {
        id: outline
        property color ink: Theme.seam
        property real cornerRadius: DesktopTokens.px(6)
        ShapePath {
            strokeColor: outline.ink; strokeWidth: 1; strokeStyle: ShapePath.DashLine
            dashPattern: [2, 2]; fillColor: "transparent"
            PathSvg {
                path: {
                    const w = outline.width - 1
                    const h = outline.height - 1
                    const r = outline.cornerRadius
                    return "M " + r + " 1 H " + (w-r) + " Q " + w + " 1 " + w + " " + r
                        + " V " + (h-r) + " Q " + w + " " + h + " " + (w-r) + " " + h
                        + " H " + r + " Q 1 " + h + " 1 " + (h-r) + " V " + r + " Q 1 1 " + r + " 1 Z"
                }
            }
        }
    }
    component Badge: Rectangle {
        property alias text: label.text
        property color ink: Theme.textMuted
        implicitWidth: label.implicitWidth + DesktopTokens.px(14)
        implicitHeight: DesktopTokens.px(24)
        radius: DesktopTokens.px(6)
        color: Qt.rgba(ink.r, ink.g, ink.b, 0.12)
        Eyebrow { id: label; anchors.centerIn: parent; color: parent.ink }
    }
    component Segments: Item {
        id: segments
        property var options: []
        property int selectedIndex: 0
        property int optionWidth: 56
        signal selected(int index, var value)
        implicitWidth: DesktopTokens.px(6) + options.length * DesktopTokens.px(optionWidth + 2)
        implicitHeight: DesktopTokens.px(36)
        Rectangle { anchors.fill: parent; radius: height / 2; color: DesktopTokens.raised; border.color: DesktopTokens.seamSoft }
        Row {
            x: DesktopTokens.px(3); y: DesktopTokens.px(3); spacing: DesktopTokens.px(2)
            Repeater {
                model: segments.options
                AbstractButton {
                    id: segment
                    required property int index
                    required property var modelData
                    objectName: "settingsOption-" + modelData.value
                    width: DesktopTokens.px(segments.optionWidth); height: DesktopTokens.px(30)
                    checkable: true; checked: index === segments.selectedIndex
                    Accessible.name: modelData.label
                    onClicked: segments.selected(index, modelData)
                    background: Rectangle { radius: height / 2; color: segment.checked ? Theme.face : segment.hovered ? DesktopTokens.raised : "transparent"; border.width: segment.activeFocus ? 2 : 0; border.color: Theme.focus }
                    contentItem: Text {
                        text: segment.modelData.label; color: segment.checked ? Theme.faceText : Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(13); font.weight: Font.ExtraBold
                        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }
    }
    component Checker: Rectangle {
        id: checker
        property bool enhanced: false
        width: DesktopTokens.px(56); height: width
        radius: DesktopTokens.px(8)
        color: DesktopTokens.raised
        border.color: Theme.seam
        Grid {
            anchors.fill: parent; anchors.margins: DesktopTokens.px(2)
            columns: checker.enhanced ? 6 : 4
            Repeater {
                model: checker.enhanced ? 36 : 16
                Rectangle {
                    required property int index
                    readonly property int count: checker.enhanced ? 6 : 4
                    width: (checker.width - DesktopTokens.px(4)) / count; height: width
                    color: checker.enhanced ? root.blue : Theme.label
                    opacity: (Math.floor(index / count) + index) % 2 === 0 ? 0.18 : 0.06
                }
            }
        }
    }
    component TuningSlider: Row {
        id: control
        property alias from: slider.from
        property alias to: slider.to
        property alias stepSize: slider.stepSize
        property alias value: slider.value
        property real trackWidth: DesktopTokens.px(150)
        property string suffix: ""
        property string accessibleName: ""
        signal moved(real value)
        spacing: DesktopTokens.px(16)
        Slider {
            id: slider
            width: control.trackWidth; height: DesktopTokens.px(28)
            leftPadding: 0; rightPadding: 0
            Accessible.name: control.accessibleName
            onMoved: control.moved(value)
            background: Rectangle {
                anchors.verticalCenter: parent.verticalCenter; width: parent.width
                height: DesktopTokens.px(4); radius: height / 2; color: DesktopTokens.raisedStrong
                Rectangle { width: slider.visualPosition * parent.width; height: parent.height; radius: height / 2; color: root.blue }
            }
            handle: Rectangle {
                x: slider.visualPosition * (slider.width - width); anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(16); height: width; radius: width / 2; color: Theme.label
                border.width: slider.activeFocus ? 2 : 0; border.color: root.blue
            }
        }
        Text {
            width: DesktopTokens.px(26); height: DesktopTokens.px(28)
            text: Math.round(slider.value) + control.suffix
            color: Theme.label; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.Bold
            verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignRight
        }
    }

    DesktopOnboardingNetwork { width: parent.width; controller: root.store.onboardingAwdlController }

    GridLayout {
        width: parent.width
        columns: root.wide ? 2 : 1
        columnSpacing: DesktopTokens.px(24); rowSpacing: DesktopTokens.px(24)

        Rectangle {
            id: generationCard
            Layout.fillWidth: true; Layout.preferredWidth: DesktopTokens.px(546); Layout.minimumWidth: 0
            Layout.alignment: Qt.AlignTop
            implicitHeight: generationContents.implicitHeight + DesktopTokens.px(4)
            color: root.panelColor; radius: DesktopTokens.px(16)
            border.width: root.settings.frameGeneration === "2x" ? DesktopTokens.px(2) : 1
            border.color: root.settings.frameGeneration === "2x" ? root.mint : Theme.seam
            Column {
                id: generationContents
                x: DesktopTokens.px(2); y: DesktopTokens.px(2); width: parent.width - DesktopTokens.px(4)
                Item {
                    width: parent.width; height: DesktopTokens.px(113)
                    RowLayout {
                        x: DesktopTokens.px(20); y: DesktopTokens.px(20); width: parent.width - DesktopTokens.px(40)
                        Eyebrow { text: qsTr("2× FRAME PATTERN"); Layout.fillWidth: true }
                        Row {
                            spacing: DesktopTokens.px(6)
                            Rectangle { width: DesktopTokens.px(10); height: width; color: DesktopTokens.raisedStrong }
                            Text { text: qsTr("stream"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10) }
                            Rectangle { width: DesktopTokens.px(10); height: width; color: "transparent"; border.color: root.mint }
                            Text { text: qsTr("generated"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10) }
                        }
                    }
                    Row {
                        x: DesktopTokens.px(20); y: DesktopTokens.px(42); width: parent.width - DesktopTokens.px(40)
                        spacing: DesktopTokens.px(5)
                        Repeater {
                            model: 12
                            Rectangle {
                                required property int index
                                width: (parent.width - DesktopTokens.px(5) * 11) / 12
                                height: DesktopTokens.px(56); radius: DesktopTokens.px(6)
                                color: index % 2 === 0 ? DesktopTokens.raisedStrong : Qt.rgba(root.mint.r, root.mint.g, root.mint.b, 0.08)
                                DashedOutline { anchors.fill: parent; visible: parent.index % 2 !== 0; ink: Qt.rgba(root.mint.r, root.mint.g, root.mint.b, 0.7) }
                            }
                        }
                    }
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                }
                Column {
                    id: generationBody
                    x: DesktopTokens.px(20); width: parent.width - DesktopTokens.px(40)
                    topPadding: DesktopTokens.px(18); bottomPadding: DesktopTokens.px(20)
                    spacing: DesktopTokens.px(14)
                    GridLayout {
                        width: parent.width; columns: width < DesktopTokens.px(450) ? 1 : 2
                        columnSpacing: DesktopTokens.px(16); rowSpacing: DesktopTokens.px(10)
                        Column {
                            Layout.fillWidth: true; Layout.minimumWidth: 0
                            spacing: DesktopTokens.px(4)
                            Flow {
                                width: parent.width; spacing: DesktopTokens.px(8)
                                Copy { text: qsTr("Frame generation"); color: Theme.label; font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black }
                                Badge { text: qsTr("EXPERIMENTAL"); ink: Theme.accentColor("amber") }
                            }
                            Copy {
                                width: parent.width
                                text: qsTr("Generate intermediate frames on your device. Needs a fast GPU and a high-refresh display. May add latency and visual artifacts.")
                            }
                        }
                        Segments {
                            objectName: "onboardingFrameGeneration"
                            Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
                            implicitHeight: DesktopTokens.px(36)
                            options: [{label:qsTr("Off"),value:"off"},{label:qsTr("2×"),value:"2x"}]
                            selectedIndex: root.settings.frameGeneration === "2x" ? 1 : 0
                            optionWidth: 56
                            onSelected: (index, item) => root.store.setOnboardingSetting("frameGeneration", item.value)
                        }
                    }
                    Row {
                        width: parent.width; spacing: DesktopTokens.px(10)
                        Repeater {
                            model: [
                                {title:qsTr("2×"),caption:qsTr("illustrated pattern"),ink:root.mint},
                                {title:qsTr("Latency"),caption:qsTr("may increase"),ink:Theme.accentColor("amber")},
                                {title:qsTr("GPU"),caption:qsTr("post-decode, on device"),ink:Theme.label}
                            ]
                            Rectangle {
                                id: tradeoff
                                required property var modelData
                                width: (parent.width - DesktopTokens.px(20)) / 3
                                height: DesktopTokens.px(54); radius: DesktopTokens.px(10)
                                color: DesktopTokens.seamSoft
                                Column {
                                    x: DesktopTokens.px(12); y: DesktopTokens.px(8); width: parent.width - DesktopTokens.px(24)
                                    spacing: DesktopTokens.px(2)
                                    Text { width: parent.width; text: tradeoff.modelData.title; color: tradeoff.modelData.ink; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.Bold; elide: Text.ElideRight }
                                    Copy { width: parent.width; text: tradeoff.modelData.caption; font.pixelSize: DesktopTokens.px(11); lineHeight: DesktopTokens.px(14); elide: Text.ElideRight; maximumLineCount: 1 }
                                }
                            }
                        }
                    }
                }
                Item {
                    width: parent.width; height: Math.max(DesktopTokens.px(59), generationNote.implicitHeight + DesktopTokens.px(26))
                    Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                    DesktopSettingsIcon { x: DesktopTokens.px(20); anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(16); height: width; glyph: "info"; ink: Theme.textMuted }
                    Copy {
                        id: generationNote
                        x: DesktopTokens.px(48); anchors.verticalCenter: parent.verticalCenter; width: parent.width - x - DesktopTokens.px(20)
                        font.pixelSize: DesktopTokens.px(12)
                        text: qsTr("Off by default. The diagram illustrates 2× generation, not measured performance. You can try this later in Stream settings.")
                    }
                }
            }
        }

        Rectangle {
            id: upscalingCard
            Layout.fillWidth: true; Layout.preferredWidth: DesktopTokens.px(546); Layout.minimumWidth: 0
            Layout.alignment: Qt.AlignTop
            implicitHeight: Math.max(DesktopTokens.px(366), root.mac ? metalContents.implicitHeight + DesktopTokens.px(2) : unavailable.implicitHeight + DesktopTokens.px(56))
            Layout.minimumHeight: root.wide ? generationCard.implicitHeight : 0
            radius: DesktopTokens.px(16); border.color: Theme.seam; border.width: root.mac ? 1 : 0
            color: root.mac ? root.panelColor : Theme.lightMode ? Theme.glass : "#800B0F1A"
            DashedOutline { anchors.fill: parent; visible: !root.mac; cornerRadius: DesktopTokens.px(16) }
            Column {
                id: metalContents
                visible: root.mac
                x: 1; y: 1; width: parent.width - 2
                Item {
                    width: parent.width; height: DesktopTokens.px(126)
                    RowLayout {
                        x: DesktopTokens.px(20); y: DesktopTokens.px(20); width: parent.width - DesktopTokens.px(40)
                        Eyebrow { text: qsTr("SPATIAL UPSCALING"); Layout.fillWidth: true }
                        Badge { text: qsTr("macOS ONLY"); ink: root.blue }
                    }
                    Row {
                        x: DesktopTokens.px(20); y: DesktopTokens.px(54); spacing: DesktopTokens.px(10)
                        Checker {}
                        Text { anchors.verticalCenter: parent.verticalCenter; text: qsTr("bilinear"); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10) }
                        Text { anchors.verticalCenter: parent.verticalCenter; text: "⟶"; color: Theme.textMuted; font.pixelSize: DesktopTokens.px(30) }
                        Checker { enhanced: true }
                        Text { anchors.verticalCenter: parent.verticalCenter; text: qsTr("MetalFX spatial"); color: root.blue; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10) }
                    }
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                }
                Item {
                    width: parent.width; implicitHeight: upscaleBody.implicitHeight + DesktopTokens.px(36)
                    GridLayout {
                        id: upscaleBody
                        x: DesktopTokens.px(20); y: DesktopTokens.px(18); width: parent.width - DesktopTokens.px(40)
                        columns: width < DesktopTokens.px(450) ? 1 : 2
                        columnSpacing: DesktopTokens.px(16); rowSpacing: DesktopTokens.px(10)
                        Column {
                            Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: DesktopTokens.px(4)
                            Copy { text: qsTr("Upscaling"); color: Theme.label; font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black }
                            Copy { width: parent.width; text: qsTr("Spatial upscaling for enlarged video. Uses extra GPU time and falls back to normal scaling when MetalFX is unavailable.") }
                        }
                        Segments {
                            objectName: "onboardingUpscaling"
                            Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
                            implicitHeight: DesktopTokens.px(36)
                            options: [{label:qsTr("Off"),value:"off"},{label:"MetalFX",value:"metalfx"}]
                            optionWidth: 70
                            selectedIndex: root.metalFx ? 1 : 0
                            onSelected: (index, item) => root.store.setOnboardingSetting("upscaling", item.value)
                        }
                    }
                }
                Repeater {
                    model: [{key:"upscalingSharpness",label:qsTr("Clarity"),description:qsTr("Sharpen details before upscaling. 0 disables."),maximum:15,fallback:10},
                        {key:"upscalingDenoise",label:qsTr("Noise reduction"),description:qsTr("Smooth noise before upscaling. 0 disables."),maximum:20,fallback:0}]
                    delegate: Item {
                        id: tuningControl
                        required property var modelData
                        width: parent.width
                        implicitHeight: Math.max(DesktopTokens.px(60), tuningRow.implicitHeight + DesktopTokens.px(24))
                        Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                        RowLayout {
                            id: tuningRow
                            x: DesktopTokens.px(20); anchors.verticalCenter: parent.verticalCenter; width: parent.width - DesktopTokens.px(40)
                            spacing: DesktopTokens.px(16)
                            enabled: root.metalFx; opacity: enabled ? 1 : 0.45
                            Column {
                                Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: DesktopTokens.px(3)
                                Copy { width: parent.width; text: tuningControl.modelData.label; color: Theme.label; font.weight: Font.ExtraBold }
                                Copy { width: parent.width; text: tuningControl.modelData.description; font.pixelSize: DesktopTokens.px(12) }
                            }
                            TuningSlider {
                                objectName: "onboarding-" + tuningControl.modelData.key
                                accessibleName: tuningControl.modelData.label
                                trackWidth: Math.max(DesktopTokens.px(70), Math.min(DesktopTokens.px(150), tuningControl.width - DesktopTokens.px(400)))
                                from: 0; to: tuningControl.modelData.maximum; stepSize: 1; suffix: ""
                                value: Number(root.settings[tuningControl.modelData.key] ?? tuningControl.modelData.fallback)
                                onMoved: value => root.store.setOnboardingSetting(tuningControl.modelData.key, Math.round(value))
                            }
                        }
                    }
                }
            }
            Column {
                id: unavailable
                visible: !root.mac
                x: DesktopTokens.px(28); anchors.verticalCenter: parent.verticalCenter
                width: parent.width - DesktopTokens.px(56)
                spacing: DesktopTokens.px(14)
                Badge { text: qsTr("NOT ON THIS PLATFORM") }
                Column {
                    width: parent.width; spacing: DesktopTokens.px(6)
                    Copy { text: qsTr("Upscaling"); font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black }
                    Copy { width: parent.width; text: qsTr("MetalFX is a macOS feature. On Windows and Linux, the GPU scales the stream normally. Enlarging a lower-resolution stream can soften the picture. Choose a resolution that matches your display on the previous step.") }
                }
                Copy { width: parent.width; text: qsTr("You can change your stream resolution later in Settings."); color: root.blue; font.weight: Font.Bold }
            }
        }
    }

    Rectangle {
        width: parent.width
        implicitHeight: Math.max(DesktopTokens.px(48), hints.implicitHeight + DesktopTokens.px(24))
        radius: DesktopTokens.px(14); color: Theme.lightMode ? Theme.glass : "#B80B0F1A"; border.color: DesktopTokens.seamSoft
        Flow {
            id: hints
            x: DesktopTokens.px(18); y: DesktopTokens.px(12); width: parent.width - DesktopTokens.px(36)
            spacing: DesktopTokens.px(12)
            Badge { text: "F3" }
            Copy { height: Math.max(DesktopTokens.px(24), implicitHeight); verticalAlignment: Text.AlignVCenter; text: qsTr("Check stream statistics mid-game to compare received and displayed FPS."); font.pixelSize: DesktopTokens.px(12); width: Math.min(implicitWidth, hints.width) }
            Badge { text: "Ctrl G" }
            Copy { height: Math.max(DesktopTokens.px(24), implicitHeight); verticalAlignment: Text.AlignVCenter; text: qsTr("Open the stream menu without leaving your game."); font.pixelSize: DesktopTokens.px(12); width: Math.min(implicitWidth, hints.width) }
        }
    }
}

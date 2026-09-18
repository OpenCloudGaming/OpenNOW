import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import OpenNOW

Column {
    id: page
    objectName: "desktopAppearanceSettings"
    required property real availableWidth
    required property var settingsScreen
    required property Component interfacePageComponent

    width: page.availableWidth; spacing: DesktopTokens.px(12)
    readonly property string backgroundImage: String(page.settingsScreen.valueSetting("desktopBackgroundImage", ""))

    FileDialog {
        id: backgroundDialog
        objectName: "customBackgroundDialog"
        title: qsTr("Choose a background image")
        fileMode: FileDialog.OpenFile
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.webp *.bmp)")]
        onAccepted: {
            page.settingsScreen.setSetting("desktopBackgroundImage", selectedFile.toString())
            page.settingsScreen.setSetting("desktopBackground", "custom")
            chooseBackgroundButton.forceActiveFocus()
        }
        onRejected: chooseBackgroundButton.forceActiveFocus()
    }
    Loader { width: parent.width; sourceComponent: page.interfacePageComponent }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("THEME") }
        DesktopSettingsChoice {
            objectName: "renewThemeChoice"
            width: parent.width; glyph: "moon"; title: qsTr("Theme")
            description: qsTr("Applies the pack's appearance, accent and surfaces")
            items: ["aurora","nocturne","kraft","phosphor","hibiscus","chapel","bone","cobalt"].map(id => ({label:page.settingsScreen.themeMeta(id).name,detail:page.settingsScreen.themeMeta(id).blurb,value:id}))
            value: page.settingsScreen.valueSetting("themePack","nocturne")
            onSelected: value => page.settingsScreen.setChoice("themePack",value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "moon"; title: qsTr("Appearance")
            DesktopSettingsSegmented {
                options: [{label:qsTr("Auto"),value:"auto"},{label:qsTr("Light"),value:"light"},{label:qsTr("Dark"),value:"dark"}]
                selectedIndex: options.findIndex(item => item.value === page.settingsScreen.valueSetting("appTheme","auto"))
                onSelected: (index,item) => page.settingsScreen.setSetting("appTheme",item.value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "palette"; title: qsTr("Accent"); description: qsTr("Selection, toggles and keyboard focus")
            Row {
                spacing: DesktopTokens.px(10)
                DesktopSettingsButton {
                    objectName: "themePackAccent"
                    text: qsTr("Theme")
                    compact: true
                    primary: !Theme.accentOverridden
                    onClicked: page.settingsScreen.setSetting("themeAccentOverride", false)
                }
                Repeater {
                    model: Theme.accentChoices
                    delegate: AbstractButton {
                        required property string modelData
                        objectName: "themeAccent-" + modelData
                        width: DesktopTokens.px(28); height: DesktopTokens.px(28); Accessible.name: modelData
                        checked: Theme.accentOverridden && Theme.accent === modelData
                        onClicked: page.settingsScreen.setChoice("appAccentColor",modelData)
                        background: Rectangle {
                            radius: DesktopTokens.px(14); color: Theme.accentColor(parent.modelData)
                            border.width: parent.activeFocus || parent.checked ? 3 : 0
                            border.color: Theme.label
                        }
                    }
                }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Translucent interface")
            description: qsTr("Use translucent shell surfaces when supported"); showDivider: false
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("translucentUI",false); onValueChangedByUser: value => page.settingsScreen.setSetting("translucentUI",value) }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("BACKGROUND") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "image"; title: qsTr("Background"); description: qsTr("Game art, a custom image, gradient or solid color")
            DesktopSettingsSegmented {
                id: backgroundSelector
                objectName: "desktopBackgroundChoice"
                options: [{label:qsTr("Game art"),value:"art"},{label:qsTr("Gradient"),value:"gradient"},{label:qsTr("Solid"),value:"solid"},{label:qsTr("Custom"),value:"custom"}]; optionWidth: 72
                selectedIndex: options.findIndex(item => item.value === page.settingsScreen.valueSetting("desktopBackground","art"))
                onSelected: (index,item) => page.settingsScreen.setSetting("desktopBackground",item.value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "image"; title: qsTr("Custom image")
            visible: page.settingsScreen.valueSetting("desktopBackground", "art") === "custom"
            description: backgroundPreview.status === Image.Error
                ? qsTr("Image unavailable. Choose another file or remove it.")
                : page.backgroundImage !== "" ? qsTr("Keep the image in its original location") : qsTr("Choose a local PNG, JPEG, WebP or BMP image")
            Row {
                spacing: DesktopTokens.px(10)
                Image {
                    id: backgroundPreview
                    objectName: "customBackgroundPreview"
                    width: DesktopTokens.px(60); height: DesktopTokens.px(40)
                    anchors.verticalCenter: parent.verticalCenter
                    visible: page.backgroundImage !== ""
                    source: page.backgroundImage
                    sourceSize: Qt.size(width * 2, height * 2)
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
                DesktopSettingsButton {
                    id: chooseBackgroundButton
                    objectName: "chooseCustomBackground"
                    text: qsTr("Choose image…")
                    onClicked: backgroundDialog.open()
                }
                DesktopSettingsButton {
                    objectName: "removeCustomBackground"
                    text: qsTr("Remove")
                    visible: page.backgroundImage !== ""
                    onClicked: {
                        page.settingsScreen.setSetting("desktopBackgroundImage", "")
                        if (page.settingsScreen.valueSetting("desktopBackground", "art") === "custom")
                            page.settingsScreen.setSetting("desktopBackground", "art")
                        backgroundSelector.focusSelectedOption()
                    }
                }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Image opacity")
            visible: page.settingsScreen.valueSetting("desktopBackground", "art") === "custom"
            description: qsTr("0% hides the image · 100% shows the full image"); showDivider: false
            DesktopSettingsSlider {
                objectName: "customBackgroundOpacity"
                enabled: page.settingsScreen.valueSetting("desktopBackground", "art") === "custom" && page.backgroundImage !== ""
                from: 0; to: 100; stepSize: 1
                value: Number(page.settingsScreen.valueSetting("desktopBackgroundOpacity", 30))
                onCommitted: value => page.settingsScreen.setSetting("desktopBackgroundOpacity", value)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("LAYOUT") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "grid"; title: qsTr("Library tiles"); description: qsTr("How much art you see per row")
            DesktopSettingsSegmented {
                options: [qsTr("Compact"),qsTr("Cozy"),qsTr("Large")]; optionWidth: 72
                selectedIndex: Number(page.settingsScreen.valueSetting("posterSizeScale",1.05)) < 1 ? 0 : Number(page.settingsScreen.valueSetting("posterSizeScale",1.05)) > 1.1 ? 2 : 1
                onSelected: index => page.settingsScreen.setSetting("posterSizeScale",[0.9,1.05,1.25][index])
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sidebar"; title: qsTr("Collapsed sidebar")
            description: qsTr("Show icons only · Ctrl B toggles")
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("desktopRailCollapsed",true); onValueChangedByUser: value => page.settingsScreen.setSetting("desktopRailCollapsed",value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sidebar"; title: qsTr("Sidebar opens on hover"); description: qsTr("Expands over the page without moving it")
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("desktopSidebarHover",true); onValueChangedByUser: value => page.settingsScreen.setSetting("desktopSidebarHover",value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "wave"; title: qsTr("Reduce motion")
            description: qsTr("Cuts parallax and cover animations · follows your OS by default"); showDivider: false
            DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("reducedMotion",false); onValueChangedByUser: value => page.settingsScreen.setSetting("reducedMotion",value) }
        }
    }
}

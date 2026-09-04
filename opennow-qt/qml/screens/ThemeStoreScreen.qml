import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    focus: true
    property int selectedIndex: 0
    property int filterIndex: 0
    property string statusMessage: qsTr("Choose a built-in theme to preview or apply")
    readonly property var filters: ["All", "Dark", "Light", "High contrast"]
    readonly property var filterLabels: [qsTr("All"), qsTr("Dark"), qsTr("Light"), qsTr("High contrast")]
    readonly property var themes: [
        {id:"nocturne", name:"Nocturne", author:"OPENNOW", category:"Dark", detail:"VIOLET GLASS", bg:"#101321", mid:"#252A48", accent:"#A78BFA"},
        {id:"aurora", name:"Aurora", author:"OPENNOW", category:"Dark", detail:"MINT AURORA", bg:"#071A20", mid:"#16434A", accent:"#6EE7B7"},
        {id:"kraft", name:"Kraft", author:"OPENNOW", category:"Dark", detail:"WARM BRASS", bg:"#211D17", mid:"#756346", accent:"#D3A85C"},
        {id:"phosphor", name:"Phosphor", author:"OPENNOW", category:"High contrast", detail:"CRT GLOW", bg:"#07120B", mid:"#12351D", accent:"#6BFF8A"},
        {id:"bone", name:"Bone", author:"OPENNOW", category:"Light", detail:"WARM MINIMAL", bg:"#EEE7DB", mid:"#A89578", accent:"#7F6A4D"},
        {id:"cobalt", name:"Cobalt", author:"OPENNOW", category:"Light", detail:"COBALT PAPER", bg:"#F7F9FF", mid:"#ADC4FF", accent:"#245BDB"},
        {id:"hibiscus", name:"Hibiscus", author:"OPENNOW", category:"Dark", detail:"ROSE GLASS", bg:"#1F0C18", mid:"#642343", accent:"#FF6F9F"},
        {id:"chapel", name:"Chapel", author:"OPENNOW", category:"Dark", detail:"GOLD GLASS", bg:"#151225", mid:"#40355D", accent:"#FFD166"}
    ]
    readonly property var visibleThemes: {
        if (root.filterIndex === 0)
            return root.themes
        const category = root.filters[root.filterIndex]
        return root.themes.filter(theme => theme.category === category)
    }
    readonly property var selectedTheme: visibleThemes.length > 0
                                         ? visibleThemes[Math.max(0, Math.min(selectedIndex, visibleThemes.length - 1))]
                                         : null

    function select(index) {
        if (visibleThemes.length === 0)
            return
        selectedIndex = Math.max(0, Math.min(index, visibleThemes.length - 1))
        grid.positionViewAtIndex(selectedIndex, GridView.Contain)
    }

    function installSelected() {
        if (!selectedTheme)
            return
        if (!ShellStore.ready) {
            statusMessage = qsTr("The OpenNOW core is still starting")
            return
        }
        ShellStore.applySetting("themePack", selectedTheme.id)
        ShellStore.previewThemePack = ""
        ShellStore.setSetting("themePack", selectedTheme.id)
        statusMessage = qsTr("%1 applied").arg(selectedTheme.name)
    }

    function togglePreview() {
        if (!selectedTheme)
            return
        ShellStore.previewThemePack = ShellStore.previewThemePack === selectedTheme.id ? "" : selectedTheme.id
        statusMessage = ShellStore.previewThemePack === ""
                      ? qsTr("Preview closed")
                      : qsTr("Previewing %1 — press A to apply").arg(selectedTheme.name)
    }

    onSelectedIndexChanged: if (selectedIndex >= 0) ShellStore.rememberFocus("theme-store", selectedIndex)
    onFilterIndexChanged: {
        selectedIndex = 0
        ShellStore.rememberFocus("theme-store-filter", filterIndex)
    }
    Component.onCompleted: {
        filterIndex = Math.max(0, Math.min(filters.length - 1, ShellStore.focusIndex("theme-store-filter")))
        selectedIndex = Math.max(0, Math.min(visibleThemes.length - 1, ShellStore.focusIndex("theme-store")))
    }
    Component.onDestruction: ShellStore.previewThemePack = ""

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Left) {
            select(selectedIndex - 1)
            event.accepted = true
        } else if (event.key === Qt.Key_Right) {
            select(selectedIndex + 1)
            event.accepted = true
        } else if (event.key === Qt.Key_Up) {
            select(selectedIndex - 4)
            event.accepted = true
        } else if (event.key === Qt.Key_Down) {
            select(selectedIndex + 4)
            event.accepted = true
        } else if (event.key === Qt.Key_Y) {
            filterIndex = (filterIndex + 1) % filters.length
            event.accepted = true
        } else if (event.key === Qt.Key_Minus) {
            togglePreview()
            event.accepted = true
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_A) {
            installSelected()
            event.accepted = true
        }
    }

    ScreenBackground { tint: root.selectedTheme ? root.selectedTheme.mid : "#17233B" }

    GlassPanel {
        x: 40
        y: 108
        width: root.width - 80
        height: root.height - 232
        panelRadius: 44

        Item {
            id: content
            anchors.fill: parent
            anchors.margins: 28

            ListView {
                id: filterList
                width: parent.width - 190
                height: 40
                orientation: ListView.Horizontal
                spacing: 10
                clip: true
                model: root.filters
                currentIndex: root.filterIndex
                delegate: Rectangle {
                    required property string modelData
                    required property int index
                    width: filterLabel.implicitWidth + 40
                    height: 40
                    radius: 20
                    color: root.filterIndex === index ? Theme.face : Qt.rgba(1, 1, 1, 0.08)
                    border.color: root.filterIndex === index ? "transparent" : Theme.seam
                    Text {
                        id: filterLabel
                        anchors.centerIn: parent
                        text: root.filterLabels[index] + (index === 0 ? " " + root.themes.length : "")
                        color: root.filterIndex === index ? Theme.faceText : Theme.label
                        font.family: Theme.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.Black
                    }
                    MouseArea { anchors.fill: parent; onClicked: root.filterIndex = index }
                }
            }

            GlassPanel {
                anchors.right: parent.right
                width: 160
                height: 40
                panelRadius: 20
                strong: true
                Text {
                    anchors.centerIn: parent
                    text: qsTr("BUILT IN")
                    color: Theme.label
                    font.family: Theme.bodyFont
                    font.pixelSize: 13
                    font.weight: Font.Bold
                }
            }

            GridView {
                id: grid
                x: 0
                y: 62
                width: parent.width
                height: parent.height - 121
                model: root.visibleThemes
                cellWidth: width / 4
                cellHeight: height / Math.max(1, Math.ceil(count / 4))
                clip: true
                interactive: count > 8
                currentIndex: root.selectedIndex
                keyNavigationWraps: false
                Accessible.name: qsTr("Theme collection")

                delegate: Item {
                    id: card
                    required property var modelData
                    required property int index
                    width: grid.cellWidth
                    height: grid.cellHeight
                    Accessible.name: qsTr("%1 theme by %2").arg(modelData.name).arg(modelData.author)
                    Accessible.description: I18n.source(modelData.detail, I18n.revision)
                        + (ShellStore.settings.themePack === modelData.id ? qsTr(". Active") : qsTr(". Press to apply"))
                    Accessible.role: Accessible.Button
                    Accessible.onPressAction: {
                        root.select(index)
                        root.installSelected()
                    }

                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 10
                        radius: 20
                        color: Qt.rgba(1, 1, 1, 0.055)
                        border.color: root.selectedIndex === index ? Theme.focus : Theme.seam
                        border.width: root.selectedIndex === index ? 4 : 1
                        scale: root.selectedIndex === index ? 1 : 0.97
                        Behavior on scale { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }

                        Rectangle {
                            id: preview
                            x: 12
                            y: 12
                            width: parent.width - 24
                            height: Math.max(82, parent.height - 68)
                            radius: 16
                            border.color: Qt.rgba(1, 1, 1, 0.16)
                            gradient: Gradient {
                                orientation: Gradient.Horizontal
                                GradientStop { position: 0; color: modelData.bg }
                                GradientStop { position: 1; color: modelData.mid }
                            }
                            Row {
                                anchors.top: parent.top
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.margins: 10
                                height: 22
                                spacing: 6
                                Rectangle { width: 18; height: 18; radius: 9; color: modelData.accent; Text { anchors.centerIn: parent; text: qsTr("Z"); color: "#0B0F1A"; font.pixelSize: 8; font.weight: Font.Black } }
                                Rectangle { width: Math.max(54, preview.width - 150); height: 18; radius: 9; color: Qt.rgba(1, 1, 1, 0.14) }
                                Rectangle { width: 62; height: 18; radius: 9; color: Qt.rgba(1, 1, 1, 0.18) }
                            }
                            Row {
                                anchors.centerIn: parent
                                spacing: 7
                                Repeater {
                                    model: 4
                                    Rectangle {
                                        required property int index
                                        width: index === 0 ? 76 : 44
                                        height: 58
                                        radius: 9
                                        color: index === 1 ? card.modelData.accent : Qt.lighter(card.modelData.mid, 1.25 + index * 0.08)
                                        border.color: Qt.rgba(1, 1, 1, 0.38)
                                        border.width: index === 1 ? 3 : 1
                                    }
                                }
                            }
                            Rectangle {
                                anchors.horizontalCenter: parent.horizontalCenter
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: 9
                                width: 120
                                height: 20
                                radius: 10
                                color: Qt.rgba(1, 1, 1, 0.16)
                                Rectangle { anchors.centerIn: parent; width: 22; height: 3; radius: 2; color: modelData.accent }
                            }
                        }

                        Column {
                            x: 14
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 10
                            spacing: 1
                            Text { text: modelData.name; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Black }
                            Text { text: qsTr("BY @") + modelData.author + " · " + I18n.source(modelData.detail, I18n.revision); color: Theme.textMuted; font.family: Theme.monoFont; font.pixelSize: 10; font.letterSpacing: 0.4 }
                        }
                        Rectangle {
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            anchors.rightMargin: 12
                            anchors.bottomMargin: 12
                            width: installedText.implicitWidth + 22
                            height: 28
                            radius: 14
                            color: ShellStore.settings.themePack === modelData.id ? modelData.accent : Qt.rgba(1, 1, 1, 0.10)
                            border.color: Theme.seam
                            Text {
                                id: installedText
                                anchors.centerIn: parent
                                text: ShellStore.settings.themePack === modelData.id ? qsTr("✓ Active") : qsTr("Apply")
                                color: ShellStore.settings.themePack === modelData.id ? "#0B0F1A" : Theme.label
                                font.family: Theme.bodyFont
                                font.pixelSize: 12
                                font.weight: Font.Black
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: root.select(index)
                            onDoubleClicked: { root.select(index); root.installSelected() }
                        }
                    }
                }
            }

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 45
                color: "transparent"
                border.color: "transparent"
                Rectangle { anchors.left: parent.left; anchors.right: parent.right; height: 1; color: Theme.seam }
                Text {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.statusMessage + qsTr("  ·  BUILT-IN OPENNOW PALETTES")
                    color: Theme.textMuted
                    font.family: Theme.monoFont
                    font.pixelSize: 11
                    font.letterSpacing: 0.7
                }
            }
        }
    }

    AppChrome {
        anchors.fill: parent
        title: qsTr("Theme store")
        currentRoute: "store"
        leftHints: [{glyph:"Y", label:qsTr("Filter")}, {glyph:"−", label:qsTr("Preview")}]
        rightHints: [{glyph:"A", label:qsTr("Apply")}, {glyph:"+", label:qsTr("Menu")}]
        onRouteRequested: route => AppController.navigate(route)
    }
}

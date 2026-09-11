import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property var variants: []
    property int currentIndex: 0
    property bool expanded: false
    property bool popupPresented: false
    property int highlightedIndex: 0
    signal variantSelected(int index)

    width: 504
    height: 56
    activeFocusOnTab: true
    z: popupPresented ? 100 : 0
    readonly property int boundedCurrentIndex: variants.length > 0
        ? Math.max(0, Math.min(variants.length - 1, currentIndex)) : 0
    readonly property var currentVariant: variants.length > 0
        ? variants[boundedCurrentIndex] : ({store:qsTr("GeForce NOW"), inLibrary:false})

    Accessible.role: Accessible.ComboBox
    Accessible.name: qsTr("Platform")
    Accessible.description: platformName(currentVariant)
        + (Boolean(currentVariant.inLibrary) ? qsTr(", owned") : qsTr(", not owned"))

    function platformName(variant) {
        return String(variant && variant.store || qsTr("Unknown platform"))
    }

    function platformGlyph(variant) {
        const name = platformName(variant).toUpperCase()
        if (name.indexOf("EPIC") >= 0) return "E"
        if (name.indexOf("UBISOFT") >= 0) return "U"
        if (name.indexOf("BATTLE") >= 0) return "B"
        if (name.indexOf("XBOX") >= 0 || name.indexOf("MICROSOFT") >= 0) return "X"
        if (name.indexOf("GOG") >= 0) return "G"
        return "S"
    }

    function platformColor(variant) {
        const glyph = platformGlyph(variant)
        if (glyph === "E") return Theme.cartEpic
        if (glyph === "U") return Theme.cartUbisoft
        if (glyph === "B") return Theme.cartBattlenet
        if (glyph === "X") return Theme.cartXbox
        if (glyph === "G") return Theme.cartGog
        return Theme.cartSteam
    }

    function openMenu() {
        if (!variants.length)
            return
        highlightedIndex = boundedCurrentIndex
        expanded = true
        forceActiveFocus()
    }

    function closeMenu() {
        expanded = false
        forceActiveFocus()
    }

    function choose(index) {
        const bounded = Math.max(0, Math.min(variants.length - 1, index))
        variantSelected(bounded)
        closeMenu()
    }

    onExpandedChanged: {
        if (expanded) {
            closeTimer.stop()
            popupPresented = true
        } else if (popupPresented) {
            closeTimer.restart()
        }
    }
    onCurrentIndexChanged: highlightedIndex = boundedCurrentIndex

    Keys.onPressed: event => {
        if (!expanded) {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter
                    || event.key === Qt.Key_Space) {
                openMenu()
                event.accepted = true
            }
            return
        }
        if (event.key === Qt.Key_Up)
            highlightedIndex = (highlightedIndex - 1 + variants.length) % variants.length
        else if (event.key === Qt.Key_Down)
            highlightedIndex = (highlightedIndex + 1) % variants.length
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter
                 || event.key === Qt.Key_Space)
            choose(highlightedIndex)
        else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)
            closeMenu()
        else
            return
        event.accepted = true
    }

    Timer {
        id: closeTimer
        interval: Theme.overlayDuration
        repeat: false
        onTriggered: root.popupPresented = false
    }

    Rectangle {
        anchors.fill: parent
        radius: 28
        color: Theme.glassStrong
        border.color: root.activeFocus || root.expanded ? Theme.focus : Theme.seam
        border.width: root.activeFocus || root.expanded ? 3 : 1

        StoreBadge {
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            storeGlyph: root.platformGlyph(root.currentVariant)
            storeColor: root.platformColor(root.currentVariant)
        }
        Column {
            x: 56
            anchors.verticalCenter: parent.verticalCenter
            spacing: 1
            Text {
                text: root.platformName(root.currentVariant)
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: 16
                font.weight: Font.Black
            }
            Text {
                text: Boolean(root.currentVariant.inLibrary)
                    ? qsTr("Owned in your library") : qsTr("Available on GeForce NOW")
                color: Boolean(root.currentVariant.inLibrary) ? Theme.mint : Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 13
                font.weight: Font.DemiBold
            }
        }
        Text {
            anchors.right: parent.right
            anchors.rightMargin: 18
            anchors.verticalCenter: parent.verticalCenter
            text: "⌄"
            color: Theme.label
            font.pixelSize: 20
            font.weight: Font.Black
            rotation: root.expanded ? 180 : 0
            Behavior on rotation { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
        }
        TapHandler { onTapped: root.expanded ? root.closeMenu() : root.openMenu() }
    }

    Rectangle {
        visible: root.popupPresented
        x: 7
        y: 70
        width: parent.width
        height: Math.min(304, 16 + root.variants.length * 56)
        radius: 26
        color: Qt.rgba(0, 0, 0, 0.4)
        opacity: root.expanded ? 1 : 0
        scale: root.expanded ? 1 : 0.96
        transformOrigin: Item.TopRight
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
    }

    GlassPanel {
        visible: root.popupPresented
        x: 0
        y: 64
        width: parent.width
        height: Math.min(304, 16 + root.variants.length * 56)
        panelRadius: 26
        strong: true
        color: "#10131C"
        opacity: root.expanded ? 1 : 0
        scale: root.expanded ? 1 : 0.96
        transformOrigin: Item.TopRight
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }

        ListView {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 2
            clip: true
            interactive: contentHeight > height
            model: root.variants
            currentIndex: root.highlightedIndex
            delegate: ItemDelegate {
                id: platformOption
                required property var modelData
                required property int index
                width: ListView.view.width
                height: 54
                padding: 0
                highlighted: index === root.highlightedIndex
                Accessible.name: root.platformName(modelData)
                Accessible.description: Boolean(modelData.inLibrary) ? qsTr("Owned") : qsTr("Not owned")
                onClicked: root.choose(index)
                background: Rectangle {
                    radius: 22
                    color: platformOption.highlighted ? Theme.face : "transparent"
                    border.color: platformOption.highlighted ? Theme.focus : "transparent"
                    border.width: platformOption.highlighted ? 2 : 0
                }
                contentItem: Item {
                    StoreBadge {
                        x: 10
                        anchors.verticalCenter: parent.verticalCenter
                        storeGlyph: root.platformGlyph(modelData)
                        storeColor: root.platformColor(modelData)
                    }
                    Text {
                        x: 52
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - x - ownership.width - 32
                        text: root.platformName(modelData)
                        color: platformOption.highlighted ? Theme.faceText : Theme.label
                        elide: Text.ElideRight
                        font.family: Theme.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.Black
                    }
                    Rectangle {
                        id: ownership
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: ownershipText.implicitWidth + 18
                        height: 30
                        radius: 15
                        color: Boolean(modelData.inLibrary)
                            ? (platformOption.highlighted ? Qt.rgba(0.04, 0.48, 0.28, 0.18) : Qt.rgba(0.43, 0.91, 0.72, 0.13))
                            : "transparent"
                        border.color: Boolean(modelData.inLibrary) ? Theme.mint : Theme.seam
                        border.width: 1
                        Text {
                            id: ownershipText
                            anchors.centerIn: parent
                            text: Boolean(modelData.inLibrary) ? qsTr("✓ Owned") : qsTr("Not owned")
                            color: Boolean(modelData.inLibrary)
                                ? (platformOption.highlighted ? Theme.faceText : Theme.mint)
                                : (platformOption.highlighted ? Qt.rgba(0.04, 0.06, 0.10, 0.6) : Theme.textMuted)
                            font.family: Theme.bodyFont
                            font.pixelSize: 12
                            font.weight: Font.Black
                        }
                    }
                }
            }
        }
    }
}

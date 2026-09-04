import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property string prefix: ""
    property var options: []
    property int currentIndex: 0
    property bool expanded: false
    property bool popupPresented: false
    property int highlightedIndex: 0
    signal optionSelected(int index)

    width: 190
    height: 56
    activeFocusOnTab: true
    z: popupPresented ? 100 : 0
    Accessible.role: Accessible.ComboBox
    Accessible.name: prefix
    Accessible.description: currentLabel()

    function boundedIndex(index) {
        return options.length > 0 ? Math.max(0, Math.min(options.length - 1, index)) : 0
    }

    function currentLabel() {
        return options.length > 0 ? String(options[boundedIndex(currentIndex)]) : qsTr("All")
    }

    function openMenu() {
        if (!options.length)
            return
        highlightedIndex = boundedIndex(currentIndex)
        expanded = true
        forceActiveFocus()
    }

    function closeMenu() {
        expanded = false
        forceActiveFocus()
    }

    function choose(index) {
        const choice = boundedIndex(index)
        optionSelected(choice)
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
    onOptionsChanged: {
        currentIndex = boundedIndex(currentIndex)
        highlightedIndex = boundedIndex(highlightedIndex)
    }

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
            highlightedIndex = (highlightedIndex - 1 + options.length) % options.length
        else if (event.key === Qt.Key_Down)
            highlightedIndex = (highlightedIndex + 1) % options.length
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
        color: root.activeFocus || root.expanded ? Theme.glassStrong : Theme.glass
        border.color: root.activeFocus || root.expanded ? Theme.focus : Theme.seam
        border.width: root.activeFocus || root.expanded ? 3 : 1
        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }

        Text {
            x: 18
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 52
            text: (root.prefix.length ? root.prefix + " · " : "") + root.currentLabel()
            color: Theme.label
            elide: Text.ElideRight
            font.family: Theme.bodyFont
            font.pixelSize: 14
            font.weight: Font.Black
        }
        Text {
            anchors.right: parent.right
            anchors.rightMargin: 17
            anchors.verticalCenter: parent.verticalCenter
            text: "⌄"
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: 18
            font.weight: Font.Black
            rotation: root.expanded ? 180 : 0
            Behavior on rotation { NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.OutCubic } }
        }
        TapHandler { onTapped: root.expanded ? root.closeMenu() : root.openMenu() }
    }

    Rectangle {
        visible: root.popupPresented
        x: 6
        y: 70
        width: Math.max(root.width, 220)
        height: Math.min(304, 16 + root.options.length * 44)
        radius: 24
        color: Qt.rgba(0, 0, 0, 0.38)
        opacity: root.expanded ? 1 : 0
        scale: root.expanded ? 1 : 0.96
        transformOrigin: Item.TopLeft
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
    }

    GlassPanel {
        visible: root.popupPresented
        x: 0
        y: 64
        width: Math.max(root.width, 220)
        height: Math.min(304, 16 + root.options.length * 44)
        panelRadius: 24
        strong: true
        color: "#10131C"
        opacity: root.expanded ? 1 : 0
        scale: root.expanded ? 1 : 0.96
        transformOrigin: Item.TopLeft
        Behavior on opacity { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic } }

        ListView {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 2
            clip: true
            model: root.options
            currentIndex: root.highlightedIndex
            interactive: contentHeight > height
            delegate: ItemDelegate {
                id: option
                required property string modelData
                required property int index
                width: ListView.view.width
                height: 42
                padding: 0
                highlighted: index === root.highlightedIndex
                onClicked: root.choose(index)
                background: Rectangle {
                    radius: 19
                    color: option.highlighted ? Theme.face : "transparent"
                    border.color: option.highlighted ? Theme.focus : "transparent"
                    border.width: option.highlighted ? 2 : 0
                }
                contentItem: Item {
                    Text {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - 52
                        text: modelData
                        color: option.highlighted ? Theme.faceText : Theme.label
                        elide: Text.ElideRight
                        font.family: Theme.bodyFont
                        font.pixelSize: 14
                        font.weight: Font.Black
                    }
                    Text {
                        visible: index === root.currentIndex
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: "✓"
                        color: option.highlighted ? Theme.faceText : Theme.mint
                        font.pixelSize: 16
                        font.weight: Font.Black
                    }
                }
            }
        }
    }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool opened: false
    property string eyebrow: ""
    property string title: ""
    property string description: ""
    property var options: []
    property int currentIndex: -1
    property int focusedIndex: 0
    property string chooseText: qsTr("Choose")
    property string dismissText: qsTr("Cancel")
    readonly property bool present: frame.present
    signal chosen(int index)
    signal dismissed()

    anchors.fill: parent
    visible: frame.present
    enabled: opened
    z: 200

    TextMetrics {
        id: currentTagMetrics
        text: qsTr("CURRENT")
        font.family: Theme.monoFont
        font.pixelSize: 13
        font.weight: Font.Bold
        font.letterSpacing: 1.2
    }

    function optionAt(index) {
        return options && index >= 0 && index < options.length ? options[index] : null
    }
    function optionEnabled(index) {
        const option = optionAt(index)
        return option !== null && option.disabled !== true
    }
    function firstEnabled() {
        for (let index = 0; index < (options ? options.length : 0); ++index)
            if (optionEnabled(index)) return index
        return -1
    }
    function step(delta) {
        const count = options ? options.length : 0
        for (let next = focusedIndex + delta; next >= 0 && next < count; next += delta) {
            if (optionEnabled(next)) {
                focusedIndex = next
                list.positionViewAtIndex(next, ListView.Contain)
                return
            }
        }
    }
    function jumpGroup(delta) {
        const count = options ? options.length : 0
        const current = optionAt(focusedIndex)
        const group = current ? String(current.group || "") : ""
        for (let next = focusedIndex + delta; next >= 0 && next < count; next += delta) {
            const option = optionAt(next)
            if (String(option.group || "") !== group && optionEnabled(next)) {
                let start = next
                if (delta < 0) {
                    const target = String(option.group || "")
                    while (start - 1 >= 0 && String(optionAt(start - 1).group || "") === target)
                        --start
                    while (!optionEnabled(start) && start < next) ++start
                }
                focusedIndex = start
                list.positionViewAtIndex(start, ListView.Contain)
                return
            }
        }
    }
    function syncFocus() {
        focusedIndex = optionEnabled(currentIndex) ? currentIndex : firstEnabled()
        if (focusedIndex >= 0)
            list.positionViewAtIndex(focusedIndex, ListView.Center)
    }
    function choose(index) {
        if (opened && optionEnabled(index))
            chosen(index)
    }

    onOpenedChanged: if (opened) {
        syncFocus()
        forceActiveFocus()
        Qt.callLater(() => { if (root.opened && !root.activeFocus) root.forceActiveFocus() })
    }
    onOptionsChanged: {
        if (!optionEnabled(focusedIndex))
            focusedIndex = optionEnabled(currentIndex) ? currentIndex : firstEnabled()
    }

    Keys.onPressed: event => {
        if (!opened)
            return
        if (event.key === Qt.Key_Up) step(-1)
        else if (event.key === Qt.Key_Down) step(1)
        else if (event.key === Qt.Key_PageUp) jumpGroup(-1)
        else if (event.key === Qt.Key_PageDown) jumpGroup(1)
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            if (!event.isAutoRepeat) choose(focusedIndex)
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) dismissed()
        }
        event.accepted = true
    }
    Keys.onReleased: event => { if (opened) event.accepted = true }

    ConsoleSheetFrame {
        id: frame
        opened: root.opened
        onScrimClicked: root.dismissed()

        Column {
            id: header
            width: parent.width
            spacing: 8
            Text {
                visible: text !== ""
                width: parent.width
                text: root.eyebrow.toUpperCase()
                color: Theme.textMuted
                elide: Text.ElideRight
                font.family: Theme.monoFont
                font.pixelSize: 14
                font.weight: Font.Bold
                font.letterSpacing: 2
            }
            Text {
                width: parent.width
                text: root.title !== "" ? root.title : qsTr("Choose a value")
                color: Theme.label
                elide: Text.ElideRight
                font.family: Theme.displayFont
                font.pixelSize: 40
                font.weight: Font.Black
                font.letterSpacing: -0.8
            }
            Text {
                visible: text !== ""
                width: parent.width
                text: root.description
                color: Theme.textMuted
                wrapMode: Text.WordWrap
                maximumLineCount: 3
                elide: Text.ElideRight
                font.family: Theme.bodyFont
                font.pixelSize: 18
                font.weight: Font.DemiBold
            }
        }

        ListView {
            id: list
            anchors.top: header.bottom
            anchors.topMargin: 22
            anchors.bottom: footer.top
            anchors.bottomMargin: 18
            width: parent.width
            clip: true
            spacing: 2
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds
            model: root.options
            highlightMoveDuration: 0
            delegate: Item {
                id: optionItem
                required property var modelData
                required property int index
                readonly property bool focused: root.focusedIndex === optionItem.index
                readonly property bool current: root.currentIndex === optionItem.index
                readonly property bool unavailable: optionItem.modelData.disabled === true
                readonly property string group: String(optionItem.modelData.group || "")
                readonly property bool startsGroup: optionItem.group !== ""
                    && (optionItem.index === 0 || String((root.options[optionItem.index - 1] || {}).group || "") !== optionItem.group)
                readonly property string detail: String(optionItem.modelData.detail || "")
                width: ListView.view.width
                height: (startsGroup ? 40 : 0) + Math.max(60, optionText.implicitHeight + 24)
                Accessible.role: Accessible.ListItem
                Accessible.name: String(optionItem.modelData.label || "")
                Accessible.description: optionItem.detail
                Accessible.selected: optionItem.current

                Row {
                    visible: optionItem.startsGroup
                    x: 20
                    y: 10
                    width: parent.width - 40
                    spacing: 12
                    Text {
                        id: groupLabel
                        anchors.verticalCenter: parent.verticalCenter
                        text: optionItem.group
                        color: Theme.textMuted
                        font.family: Theme.monoFont
                        font.pixelSize: 13
                        font.weight: Font.Bold
                        font.letterSpacing: 1.6
                    }
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - groupLabel.width - 12
                        height: 1
                        color: Theme.seam
                    }
                }

                Rectangle {
                    id: optionBox
                    y: optionItem.startsGroup ? 40 : 0
                    width: parent.width
                    height: parent.height - y
                    radius: 20
                    color: optionItem.focused ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.12) : "transparent"
                    border.width: optionItem.focused ? 3 : 0
                    border.color: Theme.face
                    Rectangle {
                        visible: optionItem.focused
                        anchors.fill: parent
                        anchors.margins: -8
                        radius: parent.radius + 8
                        color: "transparent"
                        border.width: 5
                        border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.4)
                    }

                    Text {
                        x: 20
                        anchors.verticalCenter: parent.verticalCenter
                        visible: optionItem.current
                        text: "✓"
                        color: Theme.mint
                        font.family: Theme.bodyFont
                        font.pixelSize: 22
                        font.weight: Font.Black
                    }
                    Item {
                        x: 24
                        anchors.verticalCenter: parent.verticalCenter
                        width: 14
                        height: 16
                        visible: optionItem.unavailable && !optionItem.current
                        Rectangle {
                            x: 3; y: 0; width: 8; height: 9; radius: 4
                            color: "transparent"; border.width: 2; border.color: Theme.textMuted
                        }
                        Rectangle { y: 6; width: 14; height: 10; radius: 2; color: Theme.textMuted }
                    }
                    Column {
                        id: optionText
                        x: 58
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - x - currentTag.width - 36
                        spacing: 3
                        Text {
                            width: parent.width
                            text: String(optionItem.modelData.label || "")
                            color: optionItem.unavailable ? Theme.textMuted : Theme.label
                            elide: Text.ElideRight
                            font.family: Theme.displayFont
                            font.pixelSize: 20
                            font.weight: optionItem.current || optionItem.focused ? Font.Black : Font.Bold
                        }
                        Text {
                            visible: text !== ""
                            width: parent.width
                            text: optionItem.detail
                            color: Theme.textMuted
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            font.family: Theme.bodyFont
                            font.pixelSize: 15
                            font.weight: Font.DemiBold
                        }
                    }
                    Text {
                        id: currentTag
                        anchors.right: parent.right
                        anchors.rightMargin: 20
                        anchors.verticalCenter: parent.verticalCenter
                        visible: optionItem.current
                        width: visible ? currentTagMetrics.advanceWidth + 12 : 0
                        text: currentTagMetrics.text
                        color: Theme.mint
                        font.family: Theme.monoFont
                        font.pixelSize: 13
                        font.weight: Font.Bold
                        font.letterSpacing: 1.2
                    }
                    TapHandler {
                        enabled: !optionItem.unavailable
                        onTapped: {
                            root.focusedIndex = optionItem.index
                            root.choose(optionItem.index)
                        }
                    }
                }
            }

            Text {
                anchors.centerIn: parent
                visible: list.count === 0
                text: qsTr("No choices are available yet")
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 18
                font.weight: Font.Bold
            }
        }

        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 52
            Rectangle { width: parent.width; height: 1; color: Theme.seam }
            Row {
                anchors.bottom: parent.bottom
                spacing: 24
                ControllerGlyph { glyph: "A"; label: root.chooseText; glyphSize: 30 }
                ControllerGlyph { glyph: "B"; label: root.dismissText; glyphSize: 30 }
            }
            Row {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                visible: (root.options || []).some(option => String(option.group || "") !== "")
                ControllerGlyph { glyph: "LB"; label: ""; glyphSize: 26 }
                Item { width: 6; height: 1 }
                ControllerGlyph { glyph: "RB"; label: qsTr("Jump group"); glyphSize: 26 }
            }
        }
    }
}

import QtQuick
import OpenNOW

FocusScope {
    id: root
    property bool opened: false
    property string eyebrow: ""
    property string title: ""
    property string description: ""
    property var sections: []
    property int sectionIndex: 0
    property int optionIndex: 0
    property bool optionsPane: true
    readonly property var section: sections.length
        ? sections[Math.max(0, Math.min(sectionIndex, sections.length - 1))] : null
    readonly property var options: section ? section.options || [] : []
    readonly property int ringGutter: 14
    readonly property int sectionWidth: 220
    signal chosen(int section, int option)
    signal resetRequested()
    signal dismissed()

    anchors.fill: parent
    visible: frame.present
    enabled: opened
    z: 200

    function selectSection(index) {
        if (!sections.length)
            return
        sectionIndex = Math.max(0, Math.min(sections.length - 1, index))
        optionIndex = Math.max(0, Math.min(options.length - 1, Number(section.currentIndex || 0)))
        reveal(sectionList, sectionIndex)
        reveal(optionList, optionIndex)
    }

    function reveal(list, index) {
        if (index < 0 || index >= list.count)
            return
        list.positionViewAtIndex(index, ListView.Contain)
        const item = list.itemAtIndex(index)
        if (!item)
            return
        if (item.y - ringGutter < list.contentY)
            list.contentY = item.y - ringGutter
        else if (item.y + item.height + ringGutter > list.contentY + list.height)
            list.contentY = item.y + item.height + ringGutter - list.height
    }

    function moveOption(delta) {
        if (!options.length)
            return
        optionIndex = Math.max(0, Math.min(options.length - 1, optionIndex + delta))
        reveal(optionList, optionIndex)
    }

    onOpenedChanged: {
        if (!opened)
            return
        optionsPane = true
        selectSection(sectionIndex)
        forceActiveFocus()
    }
    onOptionsChanged: optionIndex = Math.max(0, Math.min(optionIndex, options.length - 1))

    Keys.onPressed: event => {
        if (!opened)
            return
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.dismissed()
        } else if (event.key === Qt.Key_Up) {
            if (root.optionsPane) root.moveOption(-1)
            else root.selectSection(root.sectionIndex - 1)
        } else if (event.key === Qt.Key_Down) {
            if (root.optionsPane) root.moveOption(1)
            else root.selectSection(root.sectionIndex + 1)
        } else if (event.key === Qt.Key_Left) {
            root.optionsPane = false
        } else if (event.key === Qt.Key_Right) {
            root.optionsPane = true
        } else if (event.key === Qt.Key_PageUp) {
            root.selectSection(root.sectionIndex - 1)
        } else if (event.key === Qt.Key_PageDown) {
            root.selectSection(root.sectionIndex + 1)
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            if (!event.isAutoRepeat && !root.optionsPane)
                root.optionsPane = true
            else if (!event.isAutoRepeat && root.options.length)
                root.chosen(root.sectionIndex, root.optionIndex)
        } else if (event.key === Qt.Key_X) {
            if (!event.isAutoRepeat)
                root.resetRequested()
        } else if (event.key !== Qt.Key_Tab && event.key !== Qt.Key_Backtab && event.key !== Qt.Key_Y) {
            return
        }
        event.accepted = true
    }

    ConsoleSheetFrame {
        id: frame
        opened: root.opened
        contentInset: 56
        contentTop: 64
        onScrimClicked: root.dismissed()

        Column {
            id: heading
            width: parent.width
            spacing: 1
            Text {
                text: root.eyebrow
                visible: text !== ""
                color: Theme.textMuted
                font.family: Theme.monoFont
                font.pixelSize: 14
                font.weight: Font.DemiBold
                font.letterSpacing: 1.68
            }
            Text {
                width: parent.width
                text: root.title
                color: Theme.label
                elide: Text.ElideRight
                font.family: Theme.displayFont
                font.pixelSize: 40
                font.weight: Font.Black
                font.letterSpacing: -0.8
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
            Text {
                width: parent.width
                text: root.section && root.section.description ? root.section.description : root.description
                visible: text !== ""
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 18
                font.weight: Font.DemiBold
            }
        }

        ListView {
            id: sectionList
            objectName: "consoleFilterSectionList"
            x: -root.ringGutter
            y: heading.height + 27 - root.ringGutter
            width: root.sectionWidth + root.ringGutter * 2
            height: footer.y - y - 28 + root.ringGutter
            spacing: 6
            clip: true
            leftMargin: root.ringGutter
            rightMargin: root.ringGutter
            topMargin: root.ringGutter
            bottomMargin: root.ringGutter
            interactive: contentHeight + topMargin + bottomMargin > height
            model: root.sections
            currentIndex: root.sectionIndex
            highlightFollowsCurrentItem: false
            delegate: Rectangle {
                id: sectionRow
                required property var modelData
                required property int index
                readonly property bool active: index === root.sectionIndex
                readonly property bool focusedRow: active && !root.optionsPane
                width: root.sectionWidth
                height: 73
                radius: 20
                color: active ? Theme.face : "transparent"
                Accessible.role: Accessible.PageTab
                Accessible.name: modelData.title + ", " + modelData.value
                Accessible.selected: active
                FocusFrame { focused: sectionRow.focusedRow; frameRadius: 20; visible: sectionRow.focusedRow }
                Column {
                    x: 18
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 36
                    spacing: 1
                    Text {
                        width: parent.width
                        text: sectionRow.modelData.title
                        color: sectionRow.active ? Theme.faceText : Theme.label
                        elide: Text.ElideRight
                        font.family: Theme.displayFont
                        font.pixelSize: 19
                        font.weight: Font.Black
                    }
                    Text {
                        width: parent.width
                        text: sectionRow.modelData.value
                        color: sectionRow.active ? Qt.rgba(Theme.faceText.r, Theme.faceText.g, Theme.faceText.b, 0.66) : Theme.textMuted
                        elide: Text.ElideRight
                        font.family: Theme.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.Bold
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        root.selectSection(sectionRow.index)
                        root.optionsPane = false
                    }
                }
            }
        }

        Rectangle {
            x: root.sectionWidth + 24
            y: sectionList.y + root.ringGutter
            width: 1
            height: sectionList.height - root.ringGutter * 2
            color: Theme.seam
        }

        ListView {
            id: optionList
            objectName: "consoleFilterOptionList"
            x: root.sectionWidth + 35
            y: sectionList.y
            width: parent.width - x + root.ringGutter
            height: sectionList.height
            spacing: 6
            clip: true
            leftMargin: root.ringGutter
            rightMargin: root.ringGutter
            topMargin: root.ringGutter
            bottomMargin: root.ringGutter
            interactive: contentHeight + topMargin + bottomMargin > height
            model: root.options
            currentIndex: root.optionIndex
            highlightFollowsCurrentItem: false
            delegate: Item {
                id: optionRow
                required property var modelData
                required property int index
                readonly property bool current: index === Number(root.section ? root.section.currentIndex : -1)
                readonly property bool focusedRow: index === root.optionIndex
                width: ListView.view.width - root.ringGutter * 2
                height: 60
                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData.label
                Accessible.checked: current
                Rectangle {
                    anchors.fill: parent
                    radius: 20
                    color: optionRow.focusedRow && root.optionsPane
                        ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06) : "transparent"
                }
                FocusFrame {
                    visible: optionRow.focusedRow
                    focused: true
                    parked: !root.optionsPane
                    frameRadius: 20
                }
                Text {
                    x: 23
                    width: 24
                    horizontalAlignment: Text.AlignHCenter
                    anchors.verticalCenter: parent.verticalCenter
                    visible: optionRow.current
                    text: "✓"
                    color: Theme.mint
                    font.pixelSize: 20
                    font.weight: Font.Black
                }
                Row {
                    x: 61
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 14
                    ConsoleStoreMark {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: Boolean(optionRow.modelData.store)
                        store: optionRow.modelData.store || ""
                        markSize: 34
                    }
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: optionRow.modelData.icon === "all"
                        width: 34; height: 34; radius: 17
                        color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.10)
                        Grid {
                            anchors.centerIn: parent
                            columns: 2
                            spacing: 3
                            Repeater {
                                model: 4
                                Rectangle { width: 5; height: 5; radius: 1; color: Theme.label }
                            }
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, optionRow.width - 61 - 48 - (optionRow.current ? 110 : 23))
                        text: optionRow.modelData.label
                        color: Theme.label
                        elide: Text.ElideRight
                        font.family: Theme.bodyFont
                        font.pixelSize: 20
                        font.weight: optionRow.focusedRow ? Font.Black : Font.Bold
                    }
                }
                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: 23
                    anchors.verticalCenter: parent.verticalCenter
                    visible: optionRow.current
                    text: qsTr("CURRENT")
                    color: Theme.textMuted
                    font.family: Theme.monoFont
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.04
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        root.optionsPane = true
                        root.optionIndex = optionRow.index
                        root.chosen(root.sectionIndex, optionRow.index)
                    }
                }
            }
        }

        Item {
            id: footer
            y: parent.height - height
            width: parent.width
            height: 51
            Accessible.ignored: true
            Rectangle { width: parent.width; height: 1; color: Theme.seam }
            Row {
                id: footerHints
                anchors.bottom: parent.bottom
                spacing: 22
                ControllerGlyph { glyph: "A"; label: qsTr("Choose"); glyphSize: 30; spacing: 10; labelPixelSize: 17; labelWeight: Font.ExtraBold }
                ControllerGlyph { glyph: "B"; label: qsTr("Done"); glyphSize: 30; spacing: 10; labelPixelSize: 17; labelWeight: Font.ExtraBold }
                ControllerGlyph { glyph: "X"; label: qsTr("Reset all"); glyphSize: 30; spacing: 10; labelPixelSize: 17; labelWeight: Font.ExtraBold }
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: footerHints.verticalCenter
                text: qsTr("← → SWITCH PANE")
                color: Theme.textMuted
                font.family: Theme.monoFont
                font.pixelSize: 13
                font.weight: Font.DemiBold
                font.letterSpacing: 1.04
            }
        }
    }
}

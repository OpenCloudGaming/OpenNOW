import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool opened: false
    property string title: ""
    property string eyebrow: ""
    property string message: ""
    property string detail: ""
    property string safeText: qsTr("Cancel")
    property string actionText: ""
    property bool danger: true
    property string checkboxText: ""
    property bool checked: false
    property string panelSide: "right"
    property string safeButtonObjectName: ""
    property string actionButtonObjectName: ""
    property string checkboxObjectName: ""
    readonly property bool present: frame.present
    readonly property bool informational: actionText === ""
    readonly property color toneColor: danger ? Theme.coral : Theme.yellow
    readonly property color toneInk: Theme.lightMode ? Theme.accentColor(danger ? "coral" : "amber", true) : toneColor
    readonly property var focusChain: [safeButton]
        .concat(checkboxText !== "" ? [checkboxRow] : [])
        .concat(!informational ? [actionButton] : [])
    signal safeRequested()
    signal actionRequested()

    anchors.fill: parent
    visible: frame.present
    enabled: opened
    z: 210

    function focusSafe() { safeButton.forceActiveFocus() }
    function moveFocus(delta, wrap) {
        const chain = focusChain
        let index = chain.findIndex(item => item.activeFocus)
        if (index < 0) index = 0
        else if (wrap) index = (index + delta + chain.length) % chain.length
        else index = Math.max(0, Math.min(chain.length - 1, index + delta))
        chain[index].forceActiveFocus()
    }

    onOpenedChanged: if (opened) {
        headerScroll.contentY = 0
        focusSafe()
        Qt.callLater(() => { if (root.opened && !root.focusChain.some(item => item.activeFocus)) root.focusSafe() })
    }
    Component.onCompleted: if (opened) Qt.callLater(root.focusSafe)

    Keys.onPressed: event => {
        if (!opened)
            return
        if (event.key === Qt.Key_Down) moveFocus(1, false)
        else if (event.key === Qt.Key_Up) moveFocus(-1, false)
        else if (event.key === Qt.Key_Tab) moveFocus(1, true)
        else if (event.key === Qt.Key_Backtab) moveFocus(-1, true)
        else if (event.key === Qt.Key_PageDown) headerScroll.scrollBy(headerScroll.height * 0.8)
        else if (event.key === Qt.Key_PageUp) headerScroll.scrollBy(-headerScroll.height * 0.8)
        else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) safeRequested()
        }
        event.accepted = true
    }
    Keys.onReleased: event => { if (opened) event.accepted = true }

    ConsoleSheetFrame {
        id: frame
        opened: root.opened
        panelSide: root.panelSide
        panelWidth: root.panelSide === "left" ? 668 : 760
        toneColor: root.toneColor
        onScrimClicked: root.safeRequested()

        Flickable {
            id: headerScroll
            objectName: "consoleWarningSheetContent"
            width: parent.width
            anchors.top: parent.top
            anchors.bottom: actions.top
            anchors.bottomMargin: 24
            contentWidth: width
            contentHeight: header.implicitHeight
            clip: true
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds
            function scrollBy(delta) {
                contentY = Math.max(0, Math.min(Math.max(0, contentHeight - height), contentY + delta))
            }
            ScrollBar.vertical: ScrollBar { policy: headerScroll.interactive ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
            Column {
                id: header
                width: parent.width
                spacing: 24
                Rectangle {
                    width: 64; height: 64; radius: 32
                    color: root.toneColor
                    Text {
                        anchors.centerIn: parent
                        text: "!"
                        color: Theme.contrastText(root.toneColor)
                        font.family: Theme.displayFont
                        font.pixelSize: 34
                        font.weight: Font.Black
                    }
                }
                Column {
                    width: parent.width
                    spacing: 10
                    Text {
                        visible: text !== ""
                        width: parent.width
                        text: root.eyebrow.toUpperCase()
                        color: root.toneInk
                        elide: Text.ElideRight
                        font.family: Theme.monoFont
                        font.pixelSize: 14
                        font.weight: Font.Bold
                        font.letterSpacing: 2
                    }
                    Text {
                        width: parent.width
                        text: root.title
                        color: Theme.label
                        wrapMode: Text.WordWrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                        font.family: Theme.displayFont
                        font.pixelSize: 44
                        font.weight: Font.Black
                        font.letterSpacing: -0.9
                        lineHeight: 1.05
                    }
                }
                Text {
                    visible: text !== ""
                    width: parent.width
                    text: root.message
                    color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.76)
                    wrapMode: Text.WordWrap
                    font.family: Theme.bodyFont
                    font.pixelSize: 20
                    font.weight: Font.DemiBold
                    lineHeight: 1.25
                }
                Rectangle {
                    visible: root.detail !== ""
                    width: parent.width
                    height: factColumn.implicitHeight + 40
                    radius: 22
                    color: Qt.rgba(root.toneColor.r, root.toneColor.g, root.toneColor.b, 0.07)
                    border.width: 1
                    border.color: Qt.rgba(root.toneColor.r, root.toneColor.g, root.toneColor.b, 0.22)
                    Column {
                        id: factColumn
                        x: 22; y: 20
                        width: parent.width - 44
                        spacing: 10
                        Repeater {
                            model: root.detail === "" ? [] : root.detail.split("\n")
                            Row {
                                id: factRow
                                required property string modelData
                                width: factColumn.width
                                spacing: 12
                                Rectangle { y: 10; width: 6; height: 6; radius: 3; color: root.toneColor }
                                Text {
                                    width: parent.width - 18
                                    text: factRow.modelData
                                    color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.82)
                                    wrapMode: Text.WordWrap
                                    font.family: Theme.bodyFont
                                    font.pixelSize: 18
                                    font.weight: Font.Bold
                                }
                            }
                        }
                    }
                }
            }
        }

        Column {
            id: actions
            anchors.bottom: hints.top
            anchors.bottomMargin: 20
            width: parent.width
            spacing: 12

            ConsoleActionButton {
                id: safeButton
                objectName: root.safeButtonObjectName
                width: parent.width
                height: 76
                primary: true
                glyph: root.informational ? "A" : "B"
                text: root.safeText
                onClicked: if (root.opened) root.safeRequested()
                Keys.onTabPressed: event => { root.moveFocus(1, true); event.accepted = true }
                Keys.onBacktabPressed: event => { root.moveFocus(-1, true); event.accepted = true }
            }

            ItemDelegate {
                id: checkboxRow
                objectName: root.checkboxObjectName
                visible: root.checkboxText !== ""
                width: parent.width
                height: 64
                padding: 0
                focusPolicy: Qt.StrongFocus
                Accessible.role: Accessible.CheckBox
                Accessible.name: root.checkboxText
                Accessible.checked: root.checked
                onClicked: root.checked = !root.checked
                Keys.onTabPressed: event => { root.moveFocus(1, true); event.accepted = true }
                Keys.onBacktabPressed: event => { root.moveFocus(-1, true); event.accepted = true }
                Keys.onPressed: event => {
                    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space)
                        return
                    event.accepted = true
                    if (!event.isAutoRepeat) root.checked = !root.checked
                }
                Keys.onReleased: event => {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
                        event.accepted = true
                }
                background: Rectangle {
                    radius: 22
                    color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, checkboxRow.activeFocus ? 0.12 : 0.06)
                    border.width: checkboxRow.activeFocus ? 3 : 0
                    border.color: Theme.face
                    Rectangle {
                        visible: checkboxRow.activeFocus
                        anchors.fill: parent
                        anchors.margins: -9
                        radius: parent.radius + 9
                        color: "transparent"
                        border.width: 5
                        border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.55)
                    }
                }
                contentItem: Item {
                    Row {
                    x: 22
                    height: parent.height
                    spacing: 14
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 28; height: 28; radius: 8
                        color: root.checked ? Theme.mint : "transparent"
                        border.width: 2
                        border.color: root.checked ? Theme.mint : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.6)
                        Text {
                            anchors.centerIn: parent
                            visible: root.checked
                            text: "✓"
                            color: Theme.faceText
                            font.pixelSize: 18
                            font.weight: Font.Black
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.checkboxText
                        color: Theme.label
                        font.family: Theme.displayFont
                        font.pixelSize: 19
                        font.weight: Font.ExtraBold
                    }
                    }
                }
            }

            ConsoleActionButton {
                id: actionButton
                objectName: root.actionButtonObjectName
                visible: !root.informational
                width: parent.width
                height: 76
                danger: root.danger
                text: root.actionText
                onClicked: if (root.opened) root.actionRequested()
                Keys.onTabPressed: event => { root.moveFocus(1, true); event.accepted = true }
                Keys.onBacktabPressed: event => { root.moveFocus(-1, true); event.accepted = true }
            }
        }

        Row {
            id: hints
            anchors.bottom: parent.bottom
            spacing: 22
            ControllerGlyph { glyph: "A"; label: qsTr("Choose focused"); glyphSize: 28 }
            ControllerGlyph { glyph: "B"; label: root.safeText; glyphSize: 28 }
        }
    }
}

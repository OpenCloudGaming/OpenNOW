import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "coopJoinCode"
    required property var coop
    readonly property string alphabet: "23456789ABCDEFGHJKLMNPQRSTUVWXYZ"
    readonly property int columns: 8
    readonly property string compact: String(coop.enteredCode || "").toUpperCase().replace(/[^0-9A-Z]/g, "").slice(0, 6)
    property int selected: 0
    property bool pasteFailed: false
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Enter the code")
    Accessible.description: compact === "" ? qsTr("No characters entered") : compact.split("").join(" ")

    function setCode(value) {
        pasteFailed = false
        coop.enteredCode = value.length > 3 ? value.slice(0, 3) + "-" + value.slice(3) : value
    }
    function type(character) {
        if (compact.length >= 6 || alphabet.indexOf(character) < 0)
            return
        selected = alphabet.indexOf(character)
        const next = compact + character
        setCode(next)
        if (next.length === 6)
            coop.joinByCode(next)
    }
    function erase() {
        if (compact.length > 0)
            setCode(compact.slice(0, -1))
    }
    function join() {
        if (compact.length === 6)
            coop.joinByCode(compact)
    }
    function pasteLink() {
        const text = String(AppController.readClipboardText() || "")
        const link = text.match(/join\/([0-9A-Za-z-]+)/)
        const code = String(link ? link[1] : text).toUpperCase().replace(/[^0-9A-Z]/g, "")
        if (code.length !== 6 || code.split("").some(character => alphabet.indexOf(character) < 0)) {
            pasteFailed = true
            return
        }
        setCode(code)
        coop.joinByCode(code)
    }
    function typedCharacter(event) {
        if (/^[0-9A-Za-z]$/.test(event.text))
            return event.text.toUpperCase()
        const plainKey = event.text === "" && event.key !== Qt.Key_X && event.key !== Qt.Key_Y
            && ((event.key >= Qt.Key_0 && event.key <= Qt.Key_9) || (event.key >= Qt.Key_A && event.key <= Qt.Key_Z))
        return plainKey ? String.fromCharCode(event.key) : ""
    }
    function move(dx, dy) {
        const row = Math.floor(selected / columns)
        const column = selected % columns
        selected = Math.max(0, Math.min(3, row + dy)) * columns + Math.max(0, Math.min(columns - 1, column + dx))
    }
    function runHint(index) {
        if (index === 0) type(alphabet[selected])
        else if (index === 1) erase()
        else if (index === 2) pasteLink()
        else if (index === 3) join()
        else coop.closeGuestFlow()
    }

    Keys.onPressed: event => {
        const modifiers = event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)
        if (event.key === Qt.Key_Left) move(-1, 0)
        else if (event.key === Qt.Key_Right) move(1, 0)
        else if (event.key === Qt.Key_Up) move(0, -1)
        else if (event.key === Qt.Key_Down) move(0, 1)
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (!event.isAutoRepeat) type(alphabet[selected])
        } else if (event.key === Qt.Key_Backspace) erase()
        else if (event.key === Qt.Key_Escape) {
            if (compact.length > 0) erase()
            else if (!event.isAutoRepeat) coop.closeGuestFlow()
        } else if (event.key === Qt.Key_Back) coop.closeGuestFlow()
        else if (event.key === Qt.Key_Menu) join()
        else if (event.key === Qt.Key_V && modifiers === Qt.ControlModifier) pasteLink()
        else if (event.key === Qt.Key_Y && event.text === "") pasteLink()
        else if (modifiers === Qt.NoModifier && typedCharacter(event) !== "") type(typedCharacter(event))
        else {
            event.accepted = false
            return
        }
        event.accepted = true
    }

    DesktopCoopGuestText {
        x: 200; y: 210
        text: qsTr("JOIN A FRIEND'S CO-OP")
        color: "#7FD4FF"
        mono: true
        size: 11
        lineHeightPx: 14
        tracking: 0.18
    }
    DesktopCoopGuestText {
        x: 200; y: 236
        text: qsTr("Enter the code")
        size: 44
        lineHeightPx: 54
        tracking: -0.01
        font.weight: Font.Black
        Accessible.role: Accessible.Heading
    }
    DesktopCoopGuestText {
        x: 200; y: 302
        text: qsTr("No GeForce NOW account. Pick letters with your controller.")
        color: "#A3FFFFFF"
        size: 16
        lineHeightPx: 20
    }

    Row {
        x: 200; y: 360
        spacing: 10
        Repeater {
            model: 7
            delegate: Item {
                id: box
                required property int index
                readonly property bool dash: index === 3
                readonly property int position: index > 3 ? index - 1 : index
                readonly property bool filled: position < root.compact.length
                readonly property bool cursor: position === root.compact.length
                width: dash ? dashText.implicitWidth + 8 : 72
                height: 85
                Rectangle {
                    anchors.fill: parent
                    visible: !box.dash
                    radius: 12
                    color: box.cursor ? "#1A7FD4FF" : "#4D000000"
                    border.width: box.cursor ? 2 : 1
                    border.color: box.cursor ? "#7FD4FF" : "#24FFFFFF"
                }
                DesktopCoopGuestText {
                    id: dashText
                    anchors.centerIn: parent
                    text: box.dash ? "–" : box.filled ? root.compact[box.position] : box.cursor ? "|" : "·"
                    color: box.filled ? "#FFFFFF" : "#52FFFFFF"
                    mono: true
                    size: 40
                    lineHeightPx: 48
                    font.weight: box.dash ? Font.Normal : Font.Bold
                }
            }
        }
    }

    Row {
        x: 200; y: 475
        spacing: 10
        DesktopCoopGuestIcon {
            anchors.verticalCenter: parent.verticalCenter
            size: 16
            stroke: root.pasteFailed ? "#FF8A80" : "#7FD4FF"
            path: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"
        }
        DesktopCoopGuestText {
            anchors.verticalCenter: parent.verticalCenter
            text: root.pasteFailed ? qsTr("No co-op link or code on the clipboard.")
                : qsTr("Opened an invite link? The code is already filled in.")
            color: root.pasteFailed ? "#FF8A80" : "#A3FFFFFF"
            size: 14
            lineHeightPx: 18
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }
    }

    Grid {
        x: 900; y: 300
        columns: root.columns
        spacing: 8
        Repeater {
            model: root.alphabet.length
            delegate: Rectangle {
                id: key
                required property int index
                readonly property string character: root.alphabet[index]
                readonly property bool current: root.selected === index
                width: 56
                height: 52
                radius: 10
                color: current ? "#FFFFFF" : keyPointer.hovered ? "#1FFFFFFF" : "#0FFFFFFF"
                border.width: current ? 2 : 1
                border.color: current ? "#7FD4FF" : "#0FFFFFFF"
                Accessible.role: Accessible.Button
                Accessible.name: character
                DesktopCoopGuestText {
                    anchors.centerIn: parent
                    text: key.character
                    color: key.current ? "#0A0D14" : "#FFFFFF"
                    mono: true
                    size: 21
                    lineHeightPx: 26
                    font.weight: Font.Bold
                }
                HoverHandler { id: keyPointer }
                TapHandler {
                    onTapped: {
                        root.forceActiveFocus()
                        root.type(key.character)
                    }
                }
            }
        }
    }

    DesktopCoopGuestHints {
        x: 200; y: 836
        hints: [{glyph: "A", label: qsTr("Type")}, {glyph: "B", label: qsTr("Delete")}, {glyph: "Y", label: qsTr("Paste link")},
            {glyph: "MENU", label: qsTr("Join")}, {glyph: "VIEW", label: qsTr("Back")}]
        onActivated: index => root.runHint(index)
    }
}

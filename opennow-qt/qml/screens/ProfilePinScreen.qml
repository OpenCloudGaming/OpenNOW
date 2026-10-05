import QtQuick
import OpenNOW

FocusScope {
    id: root
    property string entry: ""
    readonly property string heading: ShellStore.pinMode === "unlock" ? qsTr("Unlock %1").arg(ShellStore.pinTargetName)
        : ShellStore.pinMode === "clear" ? qsTr("Remove profile PIN") : qsTr("Create a profile PIN")
    readonly property string instruction: ShellStore.pinMode === "unlock" ? qsTr("Enter the four-digit PIN to switch profiles.")
        : ShellStore.pinMode === "clear" ? qsTr("Enter the current PIN to remove the lock.") : qsTr("Choose four digits for this profile.")
    readonly property var digits: ["1","2","3","4","5","6","7","8","9","⌫","0","✓"]

    function activate(value) {
        if (value === "⌫") {
            entry = entry.slice(0, -1)
            ShellStore.pinMessage = ""
        } else if (value === "✓") {
            if (entry.length === 4)
                ShellStore.submitPin(entry)
            else
                ShellStore.pinMessage = qsTr("Enter all four digits.")
        } else if (entry.length < 4) {
            entry += value
            ShellStore.pinMessage = ""
            if (entry.length === 4)
                submitDelay.restart()
        }
    }

    function focusedKey() {
        for (let index = 0; index < keys.count; ++index)
            if (keys.itemAt(index).activeFocus) return index
        return -1
    }

    function moveFocus(key) {
        if (cancelButton.activeFocus) {
            if (key === Qt.Key_Up) keys.itemAt(10).forceActiveFocus()
            return
        }
        const index = focusedKey()
        if (index < 0) {
            keys.itemAt(0).forceActiveFocus()
            return
        }
        const column = index % 3
        const row = Math.floor(index / 3)
        if (key === Qt.Key_Left && column > 0) keys.itemAt(index - 1).forceActiveFocus()
        else if (key === Qt.Key_Right && column < 2) keys.itemAt(index + 1).forceActiveFocus()
        else if (key === Qt.Key_Up && row > 0) keys.itemAt(index - 3).forceActiveFocus()
        else if (key === Qt.Key_Down && row < 3) keys.itemAt(index + 3).forceActiveFocus()
        else if (key === Qt.Key_Down) cancelButton.forceActiveFocus()
    }

    ScreenBackground { tint: "#211D3D" }

    Column {
        x: 120
        anchors.verticalCenter: parent.verticalCenter
        width: root.width - 760 - 240
        spacing: 22
        Rectangle {
            width: 120; height: 120; radius: 60
            color: Theme.violet
            Text {
                anchors.centerIn: parent
                text: String(ShellStore.pinTargetName || "P").slice(0, 1).toUpperCase()
                color: Theme.contrastText(Theme.violet)
                font.family: Theme.displayFont; font.pixelSize: 52; font.weight: Font.Black
            }
        }
        Text {
            width: parent.width
            text: ShellStore.pinTargetName
            color: Theme.label
            elide: Text.ElideRight
            font.family: Theme.displayFont; font.pixelSize: 56; font.weight: Font.Black; font.letterSpacing: -1
        }
    }

    AppChrome { anchors.fill: parent; title: qsTr("Profile security"); currentRoute: "settings"; bottomVisible: false }

    ConsoleSheetFrame {
        opened: true
        toneColor: Theme.violet

        Column {
            width: parent.width
            spacing: 14
            Text {
                width: parent.width
                text: qsTr("Profile security")
                color: Theme.violet
                font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2
                font.capitalization: Font.AllUppercase
            }
            Text {
                width: parent.width
                text: I18n.source(root.heading, I18n.revision)
                color: Theme.label
                wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                font.family: Theme.displayFont; font.pixelSize: 44; font.weight: Font.Black; font.letterSpacing: -0.9
                lineHeight: 1.05
            }
            Text {
                width: parent.width
                text: I18n.source(root.instruction, I18n.revision)
                color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.76)
                wrapMode: Text.WordWrap
                font.family: Theme.bodyFont; font.pixelSize: 20; font.weight: Font.DemiBold
            }
            Item { width: 1; height: 10 }
            Row {
                spacing: 16
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("%1 of 4 digits entered").arg(root.entry.length)
                Repeater {
                    model: 4
                    Rectangle {
                        required property int index
                        width: 72; height: 80; radius: 22
                        color: index < root.entry.length ? Theme.face : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                        border.color: index === root.entry.length ? Theme.face : Theme.seam
                        border.width: index === root.entry.length ? 3 : 1
                        Rectangle {
                            anchors.centerIn: parent
                            visible: index < root.entry.length
                            width: 18; height: 18; radius: 9
                            color: Theme.faceText
                        }
                    }
                }
            }
            Text {
                width: parent.width; height: 30
                text: I18n.source(ShellStore.pinMessage, I18n.revision)
                color: Theme.coral
                elide: Text.ElideRight
                font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.Bold
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Grid {
                id: keypad
                columns: 3; spacing: 12
                Repeater {
                    id: keys
                    model: root.digits
                    ConsoleActionButton {
                        id: keyButton
                        required property string modelData
                        required property int index
                        width: Math.floor((keypad.parent.width - 24) / 3); height: 80
                        text: modelData
                        primary: modelData === "✓"
                        onClicked: root.activate(modelData)
                        Component.onCompleted: if (index === 0) forceActiveFocus()
                        contentItem: Text {
                            text: keyButton.text
                            color: keyButton.inkColor
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            font.family: Theme.displayFont; font.pixelSize: 30; font.weight: Font.Black
                        }
                    }
                }
            }
        }

        Column {
            anchors.bottom: parent.bottom
            width: parent.width
            spacing: 20
            ConsoleActionButton {
                id: cancelButton
                width: parent.width
                glyph: "B"
                text: qsTr("Cancel")
                onClicked: AppController.navigate("accounts")
            }
            Row {
                spacing: 22
                ControllerGlyph { glyph: "A"; label: qsTr("Select"); glyphSize: 28 }
                ControllerGlyph { glyph: "B"; label: qsTr("Cancel"); glyphSize: 28 }
            }
        }
    }

    Timer { id: submitDelay; interval: 180; onTriggered: if (root.entry.length === 4) ShellStore.submitPin(root.entry) }
    Keys.onPressed: event => {
        if (event.key >= Qt.Key_0 && event.key <= Qt.Key_9) {
            root.activate(String(event.key - Qt.Key_0)); event.accepted = true
        } else if (event.key === Qt.Key_Backspace || event.key === Qt.Key_Delete) {
            root.activate("⌫"); event.accepted = true
        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            AppController.navigate("accounts"); event.accepted = true
        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            root.moveFocus(event.key); event.accepted = true
        }
    }
}

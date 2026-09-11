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

    ScreenBackground { tint: "#211D3D" }
    GlassPanel {
        anchors.centerIn: parent; width: 690; height: 680; panelRadius: 44; strong: true
        Column {
            anchors.fill: parent; anchors.margins: 42; spacing: 18
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.source(root.heading, I18n.revision); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 34; font.weight: Font.Black }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.source(root.instruction, I18n.revision); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 16 }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter; spacing: 16
                Repeater {
                    model: 4
                    Rectangle {
                        required property int index
                        width: 56; height: 64; radius: 18
                        color: index < root.entry.length ? Theme.focus : Theme.glassStrong
                        border.color: index === root.entry.length ? Theme.focus : Theme.seam
                        border.width: index === root.entry.length ? 3 : 1
                        Text { anchors.centerIn: parent; text: index < root.entry.length ? "●" : ""; color: Theme.faceText; font.pixelSize: 18 }
                    }
                }
            }
            Text { anchors.horizontalCenter: parent.horizontalCenter; height: 24; text: I18n.source(ShellStore.pinMessage, I18n.revision); color: Theme.coral; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Bold }
            Grid {
                id: keypad
                anchors.horizontalCenter: parent.horizontalCenter; columns: 3; spacing: 10
                Repeater {
                    model: root.digits
                    GlassButton {
                        required property string modelData
                        required property int index
                        width: 150; height: 72; text: modelData; primary: modelData === "✓"
                        onClicked: root.activate(modelData)
                        Component.onCompleted: if (index === 0) forceActiveFocus()
                    }
                }
            }
            GlassButton { anchors.horizontalCenter: parent.horizontalCenter; width: 300; glyph: "B"; text: qsTr("Cancel"); onClicked: AppController.navigate("accounts") }
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
        }
    }
    AppChrome { anchors.fill: parent; title: qsTr("Profile security"); currentRoute: "settings"; bottomVisible: false }
}

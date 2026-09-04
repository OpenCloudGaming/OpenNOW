import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool bugMode: false
    property string category: "idea"
    property bool includeDiagnostics: true

    ScreenBackground { tint: "#18223B" }
    GlassPanel {
        anchors.centerIn: parent; width: Math.min(1000, parent.width - 180); height: 650; panelRadius: 42; strong: true
        Item {
            anchors.fill: parent; anchors.margins: 34
            Text { id: heading; text: root.bugMode ? qsTr("Report a bug") : qsTr("Share feedback"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 34; font.weight: Font.Black }
            Row {
                x: parent.width - 364; y: 0; spacing: 8
                GlassButton { id: feedbackTab; width: 174; text: qsTr("Feedback"); primary: !root.bugMode; onClicked: root.bugMode = false; Component.onCompleted: forceActiveFocus() }
                GlassButton { width: 174; text: qsTr("Bug report"); primary: root.bugMode; onClicked: root.bugMode = true }
            }
            Row {
                visible: !root.bugMode; x: 0; y: 62; spacing: 8
                Repeater {
                    model: [{id:"idea",name:"Idea"},{id:"bug",name:"Problem"},{id:"other",name:"Other"}]
                    GlassButton { required property var modelData; width: 160; text: modelData.name; primary: root.category === modelData.id; onClicked: root.category = modelData.id }
                }
            }
            TextField {
                id: titleField; visible: root.bugMode; x: 0; y: 66; width: parent.width; height: 54
                placeholderText: qsTr("Short title (8–120 characters)"); color: Theme.label
                font.family: Theme.bodyFont; font.pixelSize: 16; selectByMouse: true
                background: Rectangle { radius: 18; color: Theme.glass; border.color: titleField.activeFocus ? Theme.focus : Theme.seam; border.width: titleField.activeFocus ? 3 : 1 }
            }
            TextArea {
                id: messageField; x: 0; y: root.bugMode ? 134 : 128; width: parent.width; height: root.bugMode ? 310 : 360
                placeholderText: root.bugMode ? qsTr("What happened, what did you expect, and how can we reproduce it? (40–12,000 characters)") : qsTr("Tell us what would make OpenNOW better…")
                color: Theme.label; placeholderTextColor: Theme.textMuted; wrapMode: TextEdit.Wrap
                font.family: Theme.bodyFont; font.pixelSize: 16; selectByMouse: true
                background: Rectangle { radius: 22; color: Theme.glass; border.color: messageField.activeFocus ? Theme.focus : Theme.seam; border.width: messageField.activeFocus ? 3 : 1 }
            }
            GlassButton {
                visible: root.bugMode; x: 0; y: 458; width: 360
                text: (root.includeDiagnostics ? "✓  " : "") + qsTr("Attach redacted diagnostics")
                primary: root.includeDiagnostics; onClicked: root.includeDiagnostics = !root.includeDiagnostics
            }
            Text {
                x: 0; y: 514; width: parent.width - 330
                text: ShellStore.reportingMessage || (root.bugMode ? qsTr("Your account tokens and personal paths are never included.") : qsTr("Feedback is sent only when you press Submit."))
                color: ShellStore.reportingState === "error" ? Theme.coral : Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 13; wrapMode: Text.WordWrap
            }
            GlassButton {
                id: submitButton; x: parent.width - 300; y: 506; width: 300
                text: ShellStore.reportingState === "submitting" ? qsTr("Sending…") : qsTr("Submit")
                glyph: "A"; primary: true; enabled: ShellStore.reportingState !== "submitting"
                onClicked: {
                    if (root.bugMode)
                        ShellStore.submitBugReport(titleField.text, messageField.text, root.includeDiagnostics)
                    else
                        ShellStore.submitFeedback(root.category, messageField.text)
                }
            }
        }
    }
    HintBar { anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - height - 76; hints: [{glyph:"A",label:qsTr("Submit")},{glyph:"B",label:qsTr("Back")}] }
    AppChrome { anchors.fill: parent; title: root.bugMode ? qsTr("Bug report") : qsTr("Feedback"); currentRoute: "feedback"; onRouteRequested: route => AppController.navigate(route) }
}

import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool bugMode: false
    property string category: "idea"
    property bool includeDiagnostics: true
    readonly property var categories: [{id:"idea",name:qsTr("Idea")},{id:"bug",name:qsTr("Problem")},{id:"other",name:qsTr("Other")}]

    function selectedCategoryButton() {
        const index = categories.findIndex(item => item.id === category)
        return categoryButtons.itemAt(Math.max(0, index))
    }

    function leaveField(event) {
        if (event.key !== Qt.Key_Escape && event.key !== Qt.Key_Back)
            return
        feedbackTab.forceActiveFocus()
        event.accepted = true
    }

    ScreenBackground { tint: "#18223B" }
    GlassPanel {
        x: Math.round((parent.width - width) / 2); y: 124
        width: Math.min(1200, parent.width - 192); height: parent.height - 288; panelRadius: 40
        Item {
            anchors.fill: parent; anchors.margins: 48
            Text {
                anchors.left: parent.left; anchors.verticalCenter: modeRow.verticalCenter
                text: root.bugMode ? qsTr("Report a bug") : qsTr("Share feedback")
                color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black; font.letterSpacing: -0.3
            }
            Row {
                id: modeRow
                anchors.right: parent.right; spacing: 10
                ConsoleActionButton {
                    id: feedbackTab; width: 200; height: 64; text: qsTr("Feedback"); primary: !root.bugMode
                    onClicked: root.bugMode = false
                    KeyNavigation.right: bugTab
                    KeyNavigation.down: root.bugMode ? titleField : root.selectedCategoryButton()
                    Component.onCompleted: forceActiveFocus()
                }
                ConsoleActionButton {
                    id: bugTab; width: 200; height: 64; text: qsTr("Bug report"); primary: root.bugMode
                    onClicked: root.bugMode = true
                    KeyNavigation.left: feedbackTab
                    KeyNavigation.down: root.bugMode ? titleField : root.selectedCategoryButton()
                }
            }
            Row {
                id: categoryRow
                visible: !root.bugMode; y: 92; spacing: 10
                Repeater {
                    id: categoryButtons
                    model: root.categories
                    ConsoleActionButton {
                        required property var modelData
                        required property int index
                        width: 180; height: 64; text: modelData.name; primary: root.category === modelData.id
                        onClicked: root.category = modelData.id
                        KeyNavigation.left: index > 0 ? categoryButtons.itemAt(index - 1) : null
                        KeyNavigation.right: index < root.categories.length - 1 ? categoryButtons.itemAt(index + 1) : null
                        KeyNavigation.up: feedbackTab
                        KeyNavigation.down: messageField
                    }
                }
            }
            TextField {
                id: titleField; visible: root.bugMode; y: 92; width: parent.width; height: 64
                leftPadding: 22; rightPadding: 22
                placeholderText: qsTr("Short title (8–120 characters)"); color: Theme.label; placeholderTextColor: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 18; selectByMouse: true
                KeyNavigation.priority: KeyNavigation.AfterItem
                KeyNavigation.up: feedbackTab
                KeyNavigation.down: messageField
                Keys.onPressed: event => root.leaveField(event)
                background: Rectangle {
                    radius: 22; color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                    border.color: titleField.activeFocus ? Theme.face : Theme.seam; border.width: titleField.activeFocus ? 3 : 1
                    Rectangle { anchors.fill: parent; anchors.margins: -9; radius: parent.radius + 9; color: "transparent"; border.width: 5; border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.4); visible: titleField.activeFocus }
                }
            }
            TextArea {
                id: messageField; y: 180; width: parent.width
                height: parent.height - y - (root.bugMode ? 188 : 104)
                padding: 22
                placeholderText: root.bugMode ? qsTr("What happened, what did you expect, and how can we reproduce it? (40–12,000 characters)") : qsTr("Tell us what would make OpenNOW better…")
                color: Theme.label; placeholderTextColor: Theme.textMuted; wrapMode: TextEdit.Wrap
                font.family: Theme.bodyFont; font.pixelSize: 18; selectByMouse: true
                KeyNavigation.priority: KeyNavigation.AfterItem
                KeyNavigation.up: root.bugMode ? titleField : root.selectedCategoryButton()
                KeyNavigation.down: root.bugMode ? attachToggle : submitButton
                Keys.onPressed: event => root.leaveField(event)
                background: Rectangle {
                    radius: 24; color: Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.06)
                    border.color: messageField.activeFocus ? Theme.face : Theme.seam; border.width: messageField.activeFocus ? 3 : 1
                    Rectangle { anchors.fill: parent; anchors.margins: -9; radius: parent.radius + 9; color: "transparent"; border.width: 5; border.color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.4); visible: messageField.activeFocus }
                }
            }
            ConsoleActionButton {
                id: attachToggle
                visible: root.bugMode; y: messageField.y + messageField.height + 20; width: 420; height: 64
                text: qsTr("Attach redacted diagnostics")
                Accessible.role: Accessible.CheckBox
                Accessible.checked: root.includeDiagnostics
                onClicked: root.includeDiagnostics = !root.includeDiagnostics
                KeyNavigation.up: messageField
                KeyNavigation.down: submitButton
                KeyNavigation.right: submitButton
                contentItem: Item {
                    Row {
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 14
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 28; height: 28; radius: 8
                            color: root.includeDiagnostics ? Theme.mint : "transparent"
                            border.width: 2
                            border.color: root.includeDiagnostics ? Theme.mint : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.6)
                            Text { anchors.centerIn: parent; visible: root.includeDiagnostics; text: "✓"; color: Theme.faceText; font.pixelSize: 18; font.weight: Font.Black }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: attachToggle.text; color: Theme.label
                            font.family: Theme.displayFont; font.pixelSize: 20; font.weight: Font.Black
                        }
                    }
                }
            }
            Text {
                anchors.left: parent.left; anchors.right: submitButton.left; anchors.rightMargin: 32
                anchors.verticalCenter: submitButton.verticalCenter
                text: ShellStore.reportingMessage || (root.bugMode ? qsTr("Your account tokens and personal paths are never included.") : qsTr("Feedback is sent only when you press Submit."))
                color: ShellStore.reportingState === "error" ? Theme.coral : Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.DemiBold; wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                Accessible.role: ShellStore.reportingState === "error" ? Accessible.AlertMessage : Accessible.StaticText
                Accessible.name: text
            }
            ConsoleActionButton {
                id: submitButton; anchors.right: parent.right; anchors.bottom: parent.bottom; width: 300
                text: ShellStore.reportingState === "submitting" ? qsTr("Sending…") : qsTr("Submit")
                primary: true; enabled: ShellStore.reportingState !== "submitting"
                KeyNavigation.up: root.bugMode ? attachToggle : messageField
                KeyNavigation.left: root.bugMode ? attachToggle : null
                onClicked: {
                    if (root.bugMode)
                        ShellStore.submitBugReport(titleField.text, messageField.text, root.includeDiagnostics)
                    else
                        ShellStore.submitFeedback(root.category, messageField.text)
                }
            }
        }
    }
    AppChrome {
        anchors.fill: parent; title: root.bugMode ? qsTr("Bug report") : qsTr("Feedback"); currentRoute: "feedback"
        leftHints: [{glyph:"B", label:qsTr("Back")}]
        rightHints: [{glyph:"A", label:qsTr("Select")}]
        onRouteRequested: route => AppController.navigate(route)
    }
}

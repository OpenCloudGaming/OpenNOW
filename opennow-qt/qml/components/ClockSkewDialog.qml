import QtQuick
import QtQuick.Controls

Dialog {
    id: root
    objectName: "clockSkewDialog"
    required property string message
    required property bool sessionSafe
    signal dismissed()

    visible: sessionSafe && message.length > 0
    modal: true
    focus: true
    width: Math.min(parent ? parent.width - 48 : 480, 480)
    title: qsTr("Check your computer's clock")
    standardButtons: Dialog.Ok
    closePolicy: Popup.CloseOnEscape
    onAccepted: dismissed()
    onRejected: dismissed()
    onOpened: Qt.callLater(root.claimKeyboard)
    onActiveFocusChanged: if (visible && !activeFocus) Qt.callLater(root.claimKeyboard)
    function claimKeyboard() {
        const button = standardButton(Dialog.Ok)
        if (button && visible && !activeFocus)
            button.forceActiveFocus()
    }
    Shortcut { sequences: ["Return", "Enter"]; enabled: root.visible; onActivated: root.accept() }

    contentItem: Label {
        text: root.message
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        Accessible.role: Accessible.AlertMessage
        Accessible.name: text
    }
}

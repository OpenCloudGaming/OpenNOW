import QtQuick
import QtQuick.Controls

Dialog {
    id: root
    objectName: "updateFailureDialog"
    required property string failureMessage
    required property bool sessionSafe
    signal dismissed()

    visible: sessionSafe && failureMessage.length > 0
    modal: true
    focus: true
    width: Math.min(parent ? parent.width - 48 : 480, 480)
    title: qsTr("Update could not be completed")
    standardButtons: Dialog.Ok
    closePolicy: Popup.CloseOnEscape
    onAccepted: dismissed()
    onRejected: dismissed()

    contentItem: Label {
        text: root.failureMessage
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }
}

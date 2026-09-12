import QtQuick
import QtQuick.Controls
import OpenNOW

Dialog {
    id: root
    objectName: "collectionDialog"
    property string mode: "create"
    property string collectionId: ""
    property string initialName: ""
    property var game: null
    property bool submitted: false
    signal collectionOpened(string collectionId)
    anchors.centerIn: parent
    width: Math.min(parent.width - 48, 440)
    modal: true
    closePolicy: ShellStore.collectionsBusy ? Popup.NoAutoClose : Popup.CloseOnEscape
    title: mode === "delete" ? qsTr("Delete collection") : mode === "rename" ? qsTr("Rename collection") : qsTr("New collection")
    padding: 24
    palette.windowText: DesktopTokens.text
    palette.text: DesktopTokens.text
    background: Rectangle { objectName: "collectionDialogBackground"; radius: 16; color: DesktopTokens.shell; border.color: DesktopTokens.seam }
    onAboutToShow: {
        submitted = false
        ShellStore.collectionError = ""
        nameField.text = initialName
    }
    onOpened: {
        if (mode !== "delete") {
            nameField.forceActiveFocus()
            nameField.selectAll()
        }
    }

    function submit() {
        if (!confirmButton.enabled)
            return
        submitted = mode === "delete" ? ShellStore.deleteCollection(collectionId)
            : mode === "rename" ? ShellStore.renameCollection(collectionId, nameField.text)
            : ShellStore.createCollection(nameField.text, game)
    }

    Connections {
        target: ShellStore
        function onCollectionSaved(collectionId) {
            if (!root.visible || !root.submitted)
                return
            root.close()
            if (root.mode === "create")
                root.collectionOpened(collectionId)
        }
    }

    contentItem: Column {
        spacing: 16
        Text {
            width: parent.width
            text: root.mode === "delete"
                ? qsTr("Delete “%1”? Your games will stay in your library.").arg(root.initialName)
                : qsTr("Keep your games together in a folder of your own.")
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 14
        }
        TextField {
            id: nameField
            objectName: "collectionNameField"
            width: parent.width
            visible: root.mode !== "delete"
            enabled: !ShellStore.collectionsBusy
            maximumLength: 80
            placeholderText: qsTr("Collection name")
            Accessible.name: qsTr("Collection name")
            color: DesktopTokens.text
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 15
            padding: 12
            background: Rectangle { radius: 8; color: DesktopTokens.raised; border.color: nameField.activeFocus ? DesktopTokens.focus : DesktopTokens.seam }
            onTextEdited: ShellStore.collectionError = ""
            onAccepted: root.submit()
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: ShellStore.collectionError || (root.mode !== "delete" && nameField.text.trim()
                ? ShellStore.collectionNameError(nameField.text, root.collectionId) : "")
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 13
        }
    }
    footer: Row {
        spacing: 8
        padding: 16
        layoutDirection: Qt.RightToLeft
        DesktopButton {
            id: confirmButton
            objectName: "collectionConfirmButton"
            primary: root.mode !== "delete"
            danger: root.mode === "delete"
            text: ShellStore.collectionsBusy ? qsTr("Saving…") : root.mode === "delete" ? qsTr("Delete") : root.mode === "rename" ? qsTr("Save") : qsTr("Create collection")
            enabled: !ShellStore.collectionsBusy && ShellStore.ready && (root.mode === "delete"
                || ShellStore.collectionNameError(nameField.text, root.collectionId) === "")
            onClicked: root.submit()
        }
        DesktopButton {
            text: qsTr("Cancel")
            enabled: !ShellStore.collectionsBusy
            onClicked: root.close()
        }
    }
}

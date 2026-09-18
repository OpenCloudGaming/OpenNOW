import QtQuick
import QtQuick.Controls
import QtTest
import "../../qml/components"

Item {
    width: 900
    height: 600

    UpdateFailureDialog {
        id: dialog
        anchors.centerIn: parent
        failureMessage: ""
        sessionSafe: true
        onDismissed: failureMessage = ""
    }

    TestCase {
        name: "UpdateFailureDialog"
        when: windowShown

        function cleanup() {
            dialog.failureMessage = ""
            dialog.sessionSafe = true
            tryCompare(dialog, "visible", false)
        }

        function test_failure_is_readable_and_dismissible() {
            dialog.failureMessage = "Update authorization failed. No update was installed. Start a desktop authorization agent and try again."
            tryCompare(dialog, "opened", true)
            verify(dialog.contentItem.height >= dialog.contentItem.implicitHeight)
            compare(dialog.contentItem.textFormat, Text.PlainText)
            mouseClick(dialog.standardButton(Dialog.Ok))
            tryCompare(dialog, "visible", false)
            compare(dialog.failureMessage, "")
        }

        function test_active_session_defers_without_losing_the_failure() {
            dialog.sessionSafe = false
            dialog.failureMessage = "Previous installation restored"
            verify(!dialog.visible)
            dialog.sessionSafe = true
            tryCompare(dialog, "opened", true)
            dialog.sessionSafe = false
            tryCompare(dialog, "visible", false)
            compare(dialog.failureMessage, "Previous installation restored")
            dialog.sessionSafe = true
            tryCompare(dialog, "opened", true)
            keyClick(Qt.Key_Escape)
            tryCompare(dialog, "visible", false)
            compare(dialog.failureMessage, "")
        }
    }
}

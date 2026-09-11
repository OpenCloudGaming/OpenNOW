import QtQuick

Rectangle {
    id: root
    objectName: "streamInputCaptureNotice"
    property string message: ""
    visible: message.length > 0
    anchors.horizontalCenter: parent.horizontalCenter
    anchors.bottom: parent.bottom
    anchors.bottomMargin: 96
    width: Math.min(680, parent.width - 48)
    height: copy.implicitHeight + 32
    radius: 12
    color: "#F0222936"
    border.color: "#BC9554"
    border.width: 1
    Accessible.role: Accessible.AlertMessage
    Accessible.name: heading.text + ". " + detail.text

    Column {
        id: copy
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 16
        spacing: 6

        Text {
            id: heading
            width: parent.width
            text: qsTr("Relative mouse input unavailable")
            font.pixelSize: 17
            font.weight: Font.DemiBold
            color: "#FFE0A6"
            wrapMode: Text.Wrap
        }
        Text {
            id: detail
            width: parent.width
            text: root.message + "\n" + qsTr("Video and audio are still running.")
            font.pixelSize: 14
            color: "#E3E7EF"
            wrapMode: Text.Wrap
        }
    }
}

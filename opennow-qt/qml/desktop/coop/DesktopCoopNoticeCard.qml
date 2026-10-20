import QtQuick
import QtQuick.Controls
import OpenNOW

// The compact one-line notice (H3H-1 "after let in", "after decline", "two asks at once").
Rectangle {
    id: root
    property var user: null
    property bool online: false
    property string title: ""
    property string body: ""
    property url trailingIcon: ""
    property bool dismissible: false
    signal dismissed()
    signal activated()

    width: 384
    implicitHeight: Math.max(avatar.visible ? 32 : 0, labels.implicitHeight) + 26
    radius: 16
    color: "#F00E1018"
    border.width: 1
    border.color: "#24FFFFFF"
    Accessible.role: Accessible.AlertMessage
    Accessible.name: qsTr("%1. %2").arg(title).arg(body)

    TapHandler { onTapped: root.activated() }

    DesktopCoopNoticeAvatar {
        id: avatar
        visible: root.user !== null
        x: 15
        anchors.verticalCenter: parent.verticalCenter
        width: 32
        user: root.user
        online: root.online
    }
    Column {
        id: labels
        x: avatar.visible ? 59 : 15
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - x - 15 - (trailing.visible ? 28 : 0)
        spacing: 2
        Text {
            width: parent.width
            text: root.title
            color: "#FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 14
            font.weight: Font.ExtraBold
            height: 17
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            text: root.body
            color: "#A3FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 12
            lineHeightMode: Text.FixedHeight
            lineHeight: 16
            wrapMode: Text.WordWrap
        }
    }
    AbstractButton {
        id: trailing
        visible: root.dismissible || root.trailingIcon.toString() !== ""
        enabled: root.dismissible
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        width: 24
        height: 24
        focusPolicy: Qt.NoFocus
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Dismiss")
        Accessible.ignored: !root.dismissible
        onClicked: root.dismissed()
        Image {
            anchors.centerIn: parent
            width: 16
            height: 16
            source: root.dismissible ? "qrc:/qt/qml/OpenNOW/res/icons/desktop-close.svg" : root.trailingIcon
            sourceSize: Qt.size(32, 32)
            opacity: root.dismissible ? 0.5 : 1
        }
    }
}

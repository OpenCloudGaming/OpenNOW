import QtQuick
import OpenNOW

FocusScope {
    objectName: "desktopFriendsComingSoon"
    anchors.fill: parent
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Friends")
    Column {
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 440)
        spacing: 16
        DesktopGlyph { anchors.horizontalCenter: parent.horizontalCenter; width: 36; height: 36; icon: "desktop-nav-friends.svg" }
        Text {
            width: parent.width; text: qsTr("Coming soon"); horizontalAlignment: Text.AlignHCenter
            color: Theme.label; font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(24); font.weight: Font.Bold
        }
    }
}
